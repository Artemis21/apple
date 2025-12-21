use std::{collections::HashMap, fs::remove_file, path::Path, process::Command};

use inkwell::{
    AddressSpace, OptimizationLevel,
    builder::Builder,
    context::Context,
    module::Module,
    targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine},
    types::{BasicType, BasicTypeEnum},
    values::{BasicValue, BasicValueEnum, FunctionValue},
};

use crate::{
    Error, ResultExt, Span, TExpr, Type, TypeContext, TypeRef, builtins::Builtin,
    environment::DefnId, typed_ast::Expr,
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
        let val = compile_builtin(builtin, &module, &c_ctx);
        stored_vals.insert(defn, val);
    }
    let module_ty = type_to_llvm(expr.type_, expr.span, t_ctx, &c_ctx)?.fn_type(&[], false);
    let main_fn = module.add_function("main", module_ty, None);
    compile_function_body(expr, main_fn, &mut stored_vals, t_ctx, &c_ctx)?;
    module.verify().unwrap();
    link(&module, dest);
    Ok(())
}

fn link(module: &Module, dest: &Path) {
    let obj_file = dest.with_added_extension("o");
    machine()
        .write_to_file(&module, FileType::Object, &obj_file)
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
        Expr::LiteralReal(val) => Ok(c_ctx.f32_type().const_float(val as f64).into()),
        Expr::LiteralNatural(val) => Ok(c_ctx.i32_type().const_int(val as u64, false).into()),
        Expr::Call(fn_expr, arg_exprs) => {
            let closure_pair =
                compile_expr(fn_expr, stored_vals, builder, t_ctx, c_ctx)?.into_struct_value();
            let fn_ptr = closure_pair
                .get_field_at_index(0)
                .unwrap()
                .into_pointer_value();
            let capture_ptr = closure_pair
                .get_field_at_index(1)
                .unwrap()
                .as_basic_value_enum();
            let mut args = vec![capture_ptr.into()];
            let mut arg_tys = vec![capture_ptr.get_type().into()];
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
        Expr::Reference(defn) => Ok(*stored_vals.get(&defn).unwrap()),  // FIXME: polymorphism
        _ => todo!(),
    }
}

fn compile_builtin<'ctx>(
    builtin: Builtin,
    module: &Module<'ctx>,
    ctx: &'ctx Context,
) -> BasicValueEnum<'ctx> {
    let builder = ctx.create_builder();
    match builtin {
        Builtin::Add => {
            let capture = ctx.ptr_type(addr_space()).into();
            let real = ctx.f32_type().into();
            let ty = ctx.f32_type().fn_type(&[capture, real, real], false);
            let fn_val = module.add_function(builtin.name(), ty, None);
            let left = fn_val.get_nth_param(1).unwrap().into_float_value();
            let right = fn_val.get_nth_param(2).unwrap().into_float_value();
            let entry = ctx.append_basic_block(fn_val, "entry");
            builder.position_at_end(entry);
            let result = builder.build_float_add(left, right, "add").unwrap();
            builder.build_return(Some(&result.as_basic_value_enum())).unwrap();
            let fn_ptr = fn_val.as_global_value().as_pointer_value();
            let capture_ptr = ctx.ptr_type(addr_space()).const_null();
            ctx.const_struct(&[fn_ptr.into(), capture_ptr.into()], false).into()
        }
        Builtin::Mul => {
            let capture = ctx.ptr_type(addr_space()).into();
            let nat = ctx.i32_type().into();
            let ty = ctx.i32_type().fn_type(&[capture, nat, nat], false);
            let fn_val = module.add_function(builtin.name(), ty, None);
            let left = fn_val.get_nth_param(1).unwrap().into_int_value();
            let right = fn_val.get_nth_param(2).unwrap().into_int_value();
            let entry = ctx.append_basic_block(fn_val, "entry");
            builder.position_at_end(entry);
            let result = builder.build_int_mul(left, right, "mul").unwrap();
            builder.build_return(Some(&result.as_basic_value_enum())).unwrap();
            let fn_ptr = fn_val.as_global_value().as_pointer_value();
            let capture_ptr = ctx.ptr_type(addr_space()).const_null();
            ctx.const_struct(&[fn_ptr.into(), capture_ptr.into()], false).into()
        }
        _ => ctx.bool_type().const_zero().into(), // TODO: implement
    }
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
        Type::Array(_element) => {
            let ptr = c_ctx.ptr_type(addr_space()).into();
            let len = isize_type(c_ctx);
            Ok(c_ctx.struct_type(&[ptr, len], false).into())
        }
        Type::Bool => Ok(c_ctx.bool_type().into()),
        Type::Real => Ok(c_ctx.f32_type().into()),
        Type::Natural => Ok(c_ctx.i32_type().into()),
    }
}

fn addr_space() -> AddressSpace {
    0.into()
}

fn isize_type(ctx: &Context) -> BasicTypeEnum<'_> {
    ctx.ptr_sized_int_type(&machine().get_target_data(), Some(addr_space()))
        .into()
}

fn machine() -> TargetMachine {
    Target::initialize_native(&InitializationConfig::default()).unwrap();
    let triple = TargetMachine::get_default_triple();
    Target::from_triple(&triple)
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
