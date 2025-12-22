use std::{collections::HashMap, fs::remove_file, path::Path, process::Command};

use inkwell::{
    AddressSpace, FloatPredicate, IntPredicate, OptimizationLevel,
    builder::Builder,
    context::Context,
    module::{Linkage, Module},
    passes::PassBuilderOptions,
    targets::{
        CodeModel, FileType, InitializationConfig, RelocMode, Target as LlvmTarget, TargetMachine,
    },
    types::{BasicType, BasicTypeEnum, FunctionType, IntType, StructType},
    values::{BasicValue, BasicValueEnum, FunctionValue},
};

use crate::{
    Builtin, DefnId, Error, Expr, ResultExt, Span, TExpr, Target as DefnTarget, Type, TypeContext,
    TypeRef,
};

pub fn compile(
    expr: TExpr,
    builtins: Vec<(Builtin, DefnId)>,
    t_ctx: &mut TypeContext,
    dest: &Path,
) -> Result<(), Error> {
    let c_ctx = Context::create();
    let module = c_ctx.create_module("main");
    let mut stored_vals = HashMap::new();
    for (builtin, defn) in builtins {
        let val = compile_builtin(builtin, &module, t_ctx, &c_ctx);
        stored_vals.insert(defn, val);
    }
    let module_ty = type_to_llvm(expr.type_, expr.span, t_ctx, &c_ctx)?.fn_type(&[], false);
    let main_fn = module.add_function("main", module_ty, None);
    compile_function_body(expr, main_fn, &mut stored_vals, t_ctx, &c_ctx)?;
    module.print_to_stderr();
    if let Err(e) = module.verify() {
        panic!("{}", e.to_str().unwrap());
    }
    module
        .run_passes("default<O3>", &machine(), PassBuilderOptions::create())
        .unwrap();
    module.print_to_stderr();
    link(&module, dest);
    Ok(())
}

fn link(module: &Module, dest: &Path) {
    let obj_file = dest.with_added_extension("o");
    machine()
        .write_to_file(module, FileType::Object, &obj_file)
        .unwrap();
    let status = Command::new("cc")
        .arg("-o")
        .args([dest, &obj_file])
        .spawn()
        .unwrap()
        .wait()
        .unwrap()
        .code();
    assert_eq!(status, Some(0), "linking failed");
    remove_file(obj_file).unwrap();
}

fn compile_function_body<'ctx>(
    expr: TExpr,
    func: FunctionValue<'ctx>,
    stored_vals: &mut HashMap<DefnId, BasicValueEnum<'ctx>>,
    t_ctx: &TypeContext,
    c_ctx: &'ctx Context,
) -> Result<(), Error> {
    let builder = c_ctx.create_builder();
    let entry = c_ctx.append_basic_block(func, "entry");
    builder.position_at_end(entry);
    let val = compile_expr(expr, stored_vals, &builder, t_ctx, c_ctx)?;
    builder.build_return(Some(&val)).unwrap();
    Ok(())
}

fn compile_expr<'ctx>(
    expr: TExpr,
    stored_vals: &mut HashMap<DefnId, BasicValueEnum<'ctx>>,
    builder: &Builder<'ctx>,
    t_ctx: &TypeContext,
    c_ctx: &'ctx Context,
) -> Result<BasicValueEnum<'ctx>, Error> {
    match *expr.expr {
        Expr::Call(fn_expr, arg_exprs) => {
            let closure_pair =
                compile_expr(fn_expr, stored_vals, builder, t_ctx, c_ctx)?.into_struct_value();
            let fn_ptr = builder
                .build_extract_value(closure_pair, 0, "fn")
                .unwrap()
                .into_pointer_value();
            let capture = builder
                .build_extract_value(closure_pair, 1, "capture")
                .unwrap()
                .as_basic_value_enum();
            let mut args = vec![capture.into()];
            let mut arg_tys = vec![capture.get_type().into()];
            for arg_expr in arg_exprs {
                let arg = compile_expr(arg_expr, stored_vals, builder, t_ctx, c_ctx)?;
                args.push(arg.into());
                arg_tys.push(arg.get_type().into());
            }
            let ret_ty = type_to_llvm(expr.type_, expr.span, t_ctx, c_ctx)?;
            let fn_ty = ret_ty.fn_type(&arg_tys, false);
            Ok(builder
                .build_indirect_call(fn_ty, fn_ptr, &args, "")
                .unwrap()
                .try_as_basic_value()
                .unwrap_basic())
        }
        Expr::Reference(defn) => Ok(*stored_vals.get(&defn).unwrap()), // FIXME: polymorphism
        Expr::Define(target, expr) => {
            let value = compile_expr(expr, stored_vals, builder, t_ctx, c_ctx)?;
            unpack_value(&target, value, builder, stored_vals);
            Ok(unit_value(c_ctx))
        }
        Expr::Lambda {
            params,
            captures,
            body,
        } => {
            todo!()
        }
        Expr::For {
            target,
            elem_ty,
            iter,
            body,
        } => {
            let iter_val =
                compile_expr(iter, stored_vals, builder, t_ctx, c_ctx)?.into_struct_value();
            let array_ptr = builder
                .build_extract_value(iter_val, 0, "array")
                .unwrap()
                .into_pointer_value();
            let array_len = builder
                .build_extract_value(iter_val, 1, "len")
                .unwrap()
                .into_int_value();
            let start_blk = builder.get_insert_block().unwrap();
            let loop_blk = c_ctx.insert_basic_block_after(start_blk, "for");
            let after_blk = c_ctx.insert_basic_block_after(loop_blk, "after for");
            let idx_ty = isize_type(c_ctx);
            let done = builder
                .build_int_compare(IntPredicate::EQ, idx_ty.const_zero(), array_len, "empty")
                .unwrap();
            builder
                .build_conditional_branch(done, after_blk, loop_blk)
                .unwrap();
            builder.position_at_end(loop_blk);
            let idx_phi = builder.build_phi(idx_ty, "idx").unwrap();
            let idx = idx_phi.as_basic_value().into_int_value();
            let elem_ty = type_to_llvm(elem_ty, expr.span, t_ctx, c_ctx)?;
            let elem_ptr = unsafe {
                builder
                    .build_gep(elem_ty, array_ptr, &[idx], "elptr")
                    .unwrap()
            };
            let elem = builder.build_load(elem_ty, elem_ptr, "elem").unwrap();
            unpack_value(&target, elem, builder, stored_vals);
            compile_expr(body, stored_vals, builder, t_ctx, c_ctx)?;
            let inc_idx = builder
                .build_int_add(idx, idx_ty.const_int(1, false), "inc idx")
                .unwrap();
            idx_phi.add_incoming(&[(&idx_ty.const_zero(), start_blk), (&inc_idx, loop_blk)]);
            let done = builder
                .build_int_compare(IntPredicate::EQ, inc_idx, array_len, "done")
                .unwrap();
            builder
                .build_conditional_branch(done, after_blk, loop_blk)
                .unwrap();
            builder.position_at_end(after_blk);
            Ok(unit_value(c_ctx))
        }
        Expr::If { cond, then, else_ } => {
            let cond_val = compile_expr(cond, stored_vals, builder, t_ctx, c_ctx)?.into_int_value();
            let start_blk = builder.get_insert_block().unwrap();
            let then_blk = c_ctx.insert_basic_block_after(start_blk, "then");
            let else_blk = c_ctx.insert_basic_block_after(then_blk, "else");
            let after_blk = c_ctx.insert_basic_block_after(else_blk, "after if");
            builder
                .build_conditional_branch(cond_val, then_blk, else_blk)
                .unwrap();
            builder.position_at_end(then_blk);
            let then_val = compile_expr(then, stored_vals, builder, t_ctx, c_ctx)?;
            builder.build_unconditional_branch(after_blk).unwrap();
            builder.position_at_end(else_blk);
            let else_val = compile_expr(else_, stored_vals, builder, t_ctx, c_ctx)?;
            builder.build_unconditional_branch(after_blk).unwrap();
            builder.position_at_end(after_blk);
            let result = builder.build_phi(then_val.get_type(), "if result").unwrap();
            result.add_incoming(&[(&then_val, then_blk), (&else_val, else_blk)]);
            Ok(result.as_basic_value())
        }
        Expr::Block(exprs) => {
            let mut last = unit_value(c_ctx);
            for expr in exprs {
                last = compile_expr(expr, stored_vals, builder, t_ctx, c_ctx)?;
            }
            Ok(last)
        }
        Expr::Tuple(components) => {
            let struct_ty = type_to_llvm(expr.type_, expr.span, t_ctx, c_ctx)?.into_struct_type();
            let mut tuple = struct_ty.get_undef().into();
            for (i, component) in components.into_iter().enumerate() {
                let val = compile_expr(component, stored_vals, builder, t_ctx, c_ctx)?;
                tuple = builder
                    .build_insert_value(tuple, val, i as u32, "component")
                    .unwrap();
            }
            Ok(tuple.into_struct_value().into())
        }
        Expr::LiteralReal(val) => Ok(c_ctx.f32_type().const_float(val.into()).into()),
        Expr::LiteralNatural(val) => Ok(c_ctx.i32_type().const_int(val.into(), false).into()),
    }
}

fn unpack_value<'ctx>(
    target: &DefnTarget,
    value: BasicValueEnum<'ctx>,
    builder: &Builder<'ctx>,
    stored_vals: &mut HashMap<DefnId, BasicValueEnum<'ctx>>,
) {
    match target {
        DefnTarget::Ignore => {}
        DefnTarget::Symbol(id) => {
            stored_vals.insert(*id, value);
        }
        DefnTarget::Unpack(targets, _span) => {
            let value = value.into_array_value();
            for (i, target) in targets.iter().enumerate() {
                let component = builder
                    .build_extract_value(value, i as u32, "component")
                    .unwrap();
                unpack_value(target, component, builder, stored_vals);
            }
        }
    }
}

fn compile_builtin<'ctx>(
    builtin: Builtin,
    module: &Module<'ctx>,
    t_ctx: &mut TypeContext,
    c_ctx: &'ctx Context,
) -> BasicValueEnum<'ctx> {
    let fn_ty = fn_type_to_llvm(builtin.type_(t_ctx), (0..0).into(), t_ctx, c_ctx)
        .expect("builtin type should be fully specified");
    let fn_val = module.add_function(builtin.name(), fn_ty, Some(Linkage::Private));
    let entry = c_ctx.append_basic_block(fn_val, "entry");
    let builder = c_ctx.create_builder();
    builder.position_at_end(entry);
    match builtin {
        Builtin::Add => {
            let left = fn_val.get_nth_param(1).unwrap().into_float_value();
            let right = fn_val.get_nth_param(2).unwrap().into_float_value();
            let res = builder.build_float_add(left, right, "add").unwrap();
            builder.build_return(Some(&res)).unwrap();
        }
        Builtin::Mul => {
            let left = fn_val.get_nth_param(1).unwrap().into_int_value();
            let right = fn_val.get_nth_param(2).unwrap().into_int_value();
            let res = builder.build_int_mul(left, right, "mul").unwrap();
            builder.build_return(Some(&res)).unwrap();
        }
        Builtin::Lt => {
            let left = fn_val.get_nth_param(1).unwrap().into_float_value();
            let right = fn_val.get_nth_param(2).unwrap().into_float_value();
            let res = builder
                .build_float_compare(FloatPredicate::OLT, left, right, "lt")
                .unwrap();
            builder.build_return(Some(&res)).unwrap();
        }
        Builtin::Range => {
            let low = fn_val.get_nth_param(1).unwrap().into_int_value();
            let high = fn_val.get_nth_param(2).unwrap().into_int_value();
            let len = builder.build_int_sub(high, low, "len").unwrap();
            let len = builder
                .build_int_z_extend(len, isize_type(c_ctx), "len_ext")
                .unwrap();
            let empty = builder
                .build_int_compare(IntPredicate::UGE, low, high, "empty")
                .unwrap();
            let len = builder
                .build_select(empty, isize_type(c_ctx).const_zero(), len, "len_bound")
                .unwrap()
                .into_int_value();
            let nat_ty = c_ctx.i32_type();
            let array_ptr = builder
                .build_array_malloc(nat_ty, len, "heap array")
                .unwrap();
            let loop_blk = c_ctx.insert_basic_block_after(entry, "loop");
            let after = c_ctx.insert_basic_block_after(loop_blk, "after");
            builder
                .build_conditional_branch(empty, after, loop_blk)
                .unwrap();
            builder.position_at_end(loop_blk);
            let idx_phi = builder.build_phi(nat_ty, "idx").unwrap();
            let idx = idx_phi.as_basic_value().into_int_value();
            let elem = builder.build_int_add(low, idx, "elem").unwrap();
            let loc = unsafe {
                builder
                    .build_gep(nat_ty, array_ptr, &[idx], "elem loc")
                    .unwrap()
            };
            builder.build_store(loc, elem).unwrap();
            let inc = builder
                .build_int_add(idx, nat_ty.const_int(1, false), "inc idx")
                .unwrap();
            idx_phi.add_incoming(&[(&nat_ty.const_zero(), entry), (&inc, loop_blk)]);
            let inc_ext = builder
                .build_int_z_extend(inc, isize_type(c_ctx), "inc idx ext")
                .unwrap();
            let done = builder
                .build_int_compare(IntPredicate::EQ, inc_ext, len, "done")
                .unwrap();
            builder
                .build_conditional_branch(done, after, loop_blk)
                .unwrap();
            builder.position_at_end(after);
            builder
                .build_aggregate_return(&[array_ptr.into(), len.into()])
                .unwrap();
        }
        Builtin::Print => {
            let printf_ty = c_ctx
                .i32_type()
                .fn_type(&[c_ctx.ptr_type(addr_space()).into()], true);
            let printf = module.add_function("printf", printf_ty, None);
            let val = fn_val.get_nth_param(1).unwrap().into_float_value();
            let format = builder
                .build_global_string_ptr("%f\n", "printf format")
                .unwrap()
                .as_basic_value_enum();
            let val_promote = builder
                .build_float_cast(val, c_ctx.f64_type(), "promote")
                .unwrap();
            builder
                .build_call(printf, &[format.into(), val_promote.into()], "print")
                .unwrap();
            builder.build_return(Some(&unit_value(c_ctx))).unwrap();
        }
        Builtin::ToReal => {
            let nat = fn_val.get_nth_param(1).unwrap().into_int_value();
            let res = builder
                .build_unsigned_int_to_float(nat, c_ctx.f32_type(), "to_real")
                .unwrap();
            builder.build_return(Some(&res)).unwrap();
        }
        Builtin::Normal => {
            let rand_ty = c_ctx.i32_type().fn_type(&[], false);
            let rand = module.add_function("rand", rand_ty, None);
            let _mean = fn_val.get_nth_param(1).unwrap().into_float_value();
            let _stddev = fn_val.get_nth_param(2).unwrap().into_float_value();
            // TODO: actually normal dist.
            let rand_int = builder
                .build_call(rand, &[], "rand_int")
                .unwrap()
                .try_as_basic_value()
                .unwrap_basic();
            let res = builder
                .build_bit_cast(rand_int, c_ctx.f32_type(), "rand")
                .unwrap();
            builder.build_return(Some(&res)).unwrap();
        }
        _ => {
            builder.build_unreachable().unwrap(); // TODO: implement
        }
    }
    let fn_ptr = fn_val.as_global_value().as_pointer_value();
    let capture_ptr = c_ctx.ptr_type(addr_space()).const_null();
    c_ctx
        .const_struct(&[fn_ptr.into(), capture_ptr.into()], false)
        .into()
}

fn unit_value(c_ctx: &Context) -> BasicValueEnum<'_> {
    c_ctx.struct_type(&[], false).const_zero().into()
}

fn fn_type_to_llvm<'ctx>(
    type_: TypeRef,
    span: Span,
    t_ctx: &TypeContext,
    c_ctx: &'ctx Context,
) -> Result<FunctionType<'ctx>, Error> {
    let Type::Function(params, ret) = t_ctx.get(type_).error_span(span)? else {
        panic!("expected function type")
    };
    let ret_ty = type_to_llvm(ret, span, t_ctx, c_ctx)?;
    let mut param_tys = vec![c_ctx.ptr_type(addr_space()).into()]; // first argument is captures
    for param in params {
        param_tys.push(type_to_llvm(param, span, t_ctx, c_ctx)?.into());
    }
    Ok(ret_ty.fn_type(&param_tys, false))
}

fn type_to_llvm<'ctx>(
    type_: TypeRef,
    span: Span,
    t_ctx: &TypeContext,
    c_ctx: &'ctx Context,
) -> Result<BasicTypeEnum<'ctx>, Error> {
    match t_ctx.get(type_).error_span(span)? {
        Type::Function(_params, _ret) => {
            let fun = c_ctx.ptr_type(addr_space()).into();
            let capt = c_ctx.ptr_type(addr_space()).into();
            Ok(c_ctx.struct_type(&[fun, capt], false).into())
        }
        Type::Tuple(components) => {
            let comps = components
                .into_iter()
                .map(|comp| type_to_llvm(comp, span, t_ctx, c_ctx))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(c_ctx.struct_type(&comps, false).into())
        }
        Type::Array(_element) => Ok(array_ref_type(c_ctx).into()),
        Type::Bool => Ok(c_ctx.bool_type().into()),
        Type::Real => Ok(c_ctx.f32_type().into()),
        Type::Natural => Ok(c_ctx.i32_type().into()),
    }
}

fn array_ref_type(c_ctx: &Context) -> StructType<'_> {
    let ptr = c_ctx.ptr_type(addr_space()).into();
    let len = isize_type(c_ctx).into();
    c_ctx.struct_type(&[ptr, len], false)
}

fn addr_space() -> AddressSpace {
    0.into()
}

fn isize_type(ctx: &Context) -> IntType<'_> {
    ctx.ptr_sized_int_type(&machine().get_target_data(), Some(addr_space()))
}

fn machine() -> TargetMachine {
    LlvmTarget::initialize_native(&InitializationConfig::default()).unwrap();
    let triple = TargetMachine::get_default_triple();
    LlvmTarget::from_triple(&triple)
        .unwrap()
        .create_target_machine(
            &triple,
            &TargetMachine::get_host_cpu_name().to_string(),
            &TargetMachine::get_host_cpu_features().to_string(),
            OptimizationLevel::Aggressive,
            RelocMode::Default,
            CodeModel::Default,
        )
        .unwrap()
}
