mod builtins;
mod types;

use std::{collections::HashMap, fs::remove_file, path::Path, process::Command};

use inkwell::{
    IntPredicate, OptimizationLevel,
    builder::Builder,
    context::Context,
    module::{Linkage, Module},
    passes::PassBuilderOptions,
    targets::{
        CodeModel, FileType, InitializationConfig, RelocMode, Target as LlvmTarget, TargetMachine,
    },
    types::BasicType,
    values::{BasicValue, BasicValueEnum, FunctionValue},
};

use crate::{
    Builtin, Call, Definitions, DefnId, Error, Expr, For, If, Lambda, ResultExt, TExpr,
    Target as DefnTarget, TypeContext, TypeRef, compile::types::func_ref,
};

use builtins::compile_builtins;

struct CompileCtx<'ctx, 'obj>
where
    'ctx: 'obj,
{
    types: &'ctx TypeContext,
    definitions: &'ctx Definitions,
    llvm: &'ctx Context,
    builder: &'obj Builder<'ctx>,
    module: &'obj Module<'ctx>,
    named_vals: &'obj mut HashMap<DefnId, BasicValueEnum<'ctx>>,
}

pub fn compile(
    expr: TExpr,
    builtins: &[(Builtin, DefnId)],
    types: &TypeContext,
    definitions: &Definitions,
    dest: &Path,
) -> Result<(), Error> {
    let llvm = Context::create();
    let module = llvm.create_module("main");
    let builder = llvm.create_builder();
    let mut named_vals = HashMap::new();
    let mut ctx = CompileCtx {
        types,
        definitions,
        llvm: &llvm,
        builder: &builder,
        module: &module,
        named_vals: &mut named_vals,
    };
    compile_builtins(builtins, &mut ctx);
    ctx.compile_module(expr)?;
    link(ctx.module, dest);
    Ok(())
}

impl<'ctx> CompileCtx<'ctx, '_> {
    fn compile_module(&mut self, expr: TExpr) -> Result<(), Error> {
        let module_ty = types::to_llvm(expr.type_, self)
            .error_span(expr.span)?
            .fn_type(&[], false);
        let main_fn = self.module.add_function("main", module_ty, None);
        let main_lambda = Lambda {
            params: vec![],
            captures: vec![],
            body: expr,
        };
        self.compile_function_body(main_lambda, main_fn)?;
        self.module.print_to_stderr();
        if let Err(e) = self.module.verify() {
            panic!("{}", e.to_str().unwrap());
        }
        self.module
            .run_passes("default<O3>", &machine(), PassBuilderOptions::create())
            .unwrap();
        self.module.print_to_stderr();
        Ok(())
    }

    fn compile_function_body(
        &mut self,
        Lambda {
            params,
            captures,
            body,
        }: Lambda,
        func: FunctionValue<'ctx>,
    ) -> Result<(), Error> {
        let original_block = self.builder.get_insert_block();
        let entry = self.llvm.append_basic_block(func, "entry");
        self.builder.position_at_end(entry);
        for (i, defn) in captures.into_iter().enumerate() {
            let capture = func.get_nth_param(0).unwrap().into_struct_value();
            let val = self
                .builder
                .build_extract_value(capture, i as u32, "capture")
                .unwrap();
            self.named_vals.insert(defn, val);
        }
        for (i, target) in params.into_iter().enumerate() {
            let val = func.get_nth_param(i as u32 + 1).unwrap();
            self.unpack_value(&target, val);
        }
        let val = self.compile_expr(body)?;
        self.builder.build_return(Some(&val)).unwrap();
        if let Some(block) = original_block {
            self.builder.position_at_end(block);
        }
        Ok(())
    }

    fn compile_expr(
        &mut self,
        TExpr { type_, expr, span }: TExpr,
    ) -> Result<BasicValueEnum<'ctx>, Error> {
        match *expr {
            Expr::Call(call) => self.compile_call(call, type_),
            Expr::Reference(defn) => Ok(self.compile_reference(defn)),
            Expr::Define(target, expr) => {
                let value = self.compile_expr(expr)?;
                self.unpack_value(&target, value);
                Ok(unit_value(self.llvm))
            }
            Expr::Lambda(lambda) => self.compile_lambda(lambda, type_),
            Expr::For(for_) => self.compile_for(for_),
            Expr::If(if_) => self.compile_if(if_),
            Expr::Block(exprs) => {
                let mut last = unit_value(self.llvm);
                for expr in exprs {
                    last = self.compile_expr(expr)?;
                }
                Ok(last)
            }
            Expr::Tuple(components) => self.compile_tuple(type_, components)?,
            Expr::LiteralReal(val) => Ok(self.llvm.f32_type().const_float(val.into()).into()),
            Expr::LiteralNatural(val) => {
                Ok(self.llvm.i32_type().const_int(val.into(), false).into())
            }
        }
        .error_span(span)
    }

    fn compile_call(
        &mut self,
        Call {
            callee,
            args: arg_exprs,
        }: Call,
        type_: TypeRef,
    ) -> Result<BasicValueEnum<'ctx>, Error> {
        let closure_pair = self.compile_expr(callee)?.into_struct_value();
        let fn_ptr = self
            .builder
            .build_extract_value(closure_pair, 0, "fn")
            .unwrap()
            .into_pointer_value();
        let capture = self
            .builder
            .build_extract_value(closure_pair, 1, "capture")
            .unwrap()
            .as_basic_value_enum();
        let mut args = vec![capture.into()];
        let mut arg_tys = vec![capture.get_type().into()];
        for arg_expr in arg_exprs {
            let arg = self.compile_expr(arg_expr)?;
            args.push(arg.into());
            arg_tys.push(arg.get_type().into());
        }
        let ret_ty = types::to_llvm(type_, self)?;
        let fn_ty = ret_ty.fn_type(&arg_tys, false);
        Ok(self
            .builder
            .build_indirect_call(fn_ty, fn_ptr, &args, "")
            .unwrap()
            .try_as_basic_value()
            .unwrap_basic())
    }

    fn compile_reference(&self, defn: DefnId) -> BasicValueEnum<'ctx> {
        *self.named_vals.get(&defn).unwrap() // FIXME: polymorphism
    }

    fn compile_lambda(
        &mut self,
        lambda_expr: Lambda,
        type_: TypeRef,
    ) -> Result<BasicValueEnum<'ctx>, Error> {
        // build capture struct
        let capture_tys = lambda_expr
            .captures
            .iter() // FIXME: polymorphism
            .map(|defn| types::to_llvm(self.definitions.get_type(*defn).term, self))
            .collect::<Result<Vec<_>, _>>()?;
        let capture_ty = self.llvm.struct_type(&capture_tys, false);
        let mut capture = capture_ty.get_undef().into();
        for (i, defn) in lambda_expr.captures.iter().enumerate() {
            let val = self.compile_reference(*defn);
            capture = self
                .builder
                .build_insert_value(capture, val, i as u32, "lambda_capture")
                .unwrap();
        }
        let capture_ptr = self
            .builder
            .build_alloca(capture_ty, "lambda_capture_ptr")
            .unwrap();
        self.builder.build_store(capture_ptr, capture).unwrap();
        let current_block = self.builder.get_insert_block().unwrap();

        // build function taking capture struct ptr
        let func = self.module.add_function(
            "lambda",
            types::fn_to_llvm(type_, self)?,
            Some(Linkage::Private),
        );
        self.compile_function_body(lambda_expr, func)?;

        // evaluate to { struct ptr, fn ptr }
        self.builder.position_at_end(current_block);
        let fn_ptr = func.as_global_value().as_pointer_value();
        let lambda_ty = func_ref(self.llvm);
        let mut lambda = lambda_ty.get_undef().into();
        lambda = self
            .builder
            .build_insert_value(lambda, fn_ptr, 0, "lambda")
            .unwrap();
        lambda = self
            .builder
            .build_insert_value(lambda, capture_ptr, 1, "lambda")
            .unwrap();
        Ok(lambda.into_struct_value().into())
    }

    fn compile_for(
        &mut self,
        For {
            target,
            elem_ty,
            iter,
            body: body_expr,
        }: For,
    ) -> Result<BasicValueEnum<'ctx>, Error> {
        let start = self.builder.get_insert_block().unwrap();
        let head = self.llvm.insert_basic_block_after(start, "for_head");
        let body = self.llvm.insert_basic_block_after(head, "for_body");
        let tail = self.llvm.insert_basic_block_after(body, "for_tail");

        // start: load array to iterate
        let iter_val = self.compile_expr(iter)?.into_struct_value();
        let array_ptr = self
            .builder
            .build_extract_value(iter_val, 0, "array")
            .unwrap()
            .into_pointer_value();
        let array_len = self
            .builder
            .build_extract_value(iter_val, 1, "len")
            .unwrap()
            .into_int_value();
        let idx_ty = types::isize(self.llvm);
        self.builder.build_unconditional_branch(head).unwrap();

        // head: check if we're done, then go to body or tail
        self.builder.position_at_end(head);
        let idx_phi = self.builder.build_phi(idx_ty, "idx").unwrap();
        let idx = idx_phi.as_basic_value().into_int_value();
        let continue_ = self
            .builder
            .build_int_compare(IntPredicate::ULT, idx, array_len, "continue")
            .unwrap();
        self.builder
            .build_conditional_branch(continue_, body, tail)
            .unwrap();

        // body: get element, run body expr, increment idx
        self.builder.position_at_end(body);
        let elem_ty = types::to_llvm(elem_ty, self)?;
        let elem_ptr = unsafe {
            self.builder
                .build_gep(elem_ty, array_ptr, &[idx], "elptr")
                .unwrap()
        };
        let elem = self.builder.build_load(elem_ty, elem_ptr, "elem").unwrap();
        self.unpack_value(&target, elem);
        self.compile_expr(body_expr)?;
        let inc_idx = self
            .builder
            .build_int_add(idx, idx_ty.const_int(1, false), "inc idx")
            .unwrap();
        idx_phi.add_incoming(&[(&idx_ty.const_zero(), start), (&inc_idx, body)]);
        self.builder.build_unconditional_branch(head).unwrap();

        // tail: continue executing
        self.builder.position_at_end(tail);
        Ok(unit_value(self.llvm))
    }

    fn compile_if(&mut self, If { cond, then, else_ }: If) -> Result<BasicValueEnum<'ctx>, Error> {
        let start = self.builder.get_insert_block().unwrap();
        let then_blk = self.llvm.insert_basic_block_after(start, "then");
        let else_blk = self.llvm.insert_basic_block_after(then_blk, "else");
        let after = self.llvm.insert_basic_block_after(else_blk, "after if");

        let cond_val = self.compile_expr(cond)?.into_int_value();
        self.builder
            .build_conditional_branch(cond_val, then_blk, else_blk)
            .unwrap();

        self.builder.position_at_end(then_blk);
        let then_val = self.compile_expr(then)?;
        self.builder.build_unconditional_branch(after).unwrap();

        self.builder.position_at_end(else_blk);
        let else_val = self.compile_expr(else_)?;
        self.builder.build_unconditional_branch(after).unwrap();

        self.builder.position_at_end(after);
        let result = self
            .builder
            .build_phi(then_val.get_type(), "if result")
            .unwrap();
        result.add_incoming(&[(&then_val, then_blk), (&else_val, else_blk)]);
        Ok(result.as_basic_value())
    }

    fn compile_tuple(
        &mut self,
        type_: TypeRef,
        components: Vec<TExpr>,
    ) -> Result<Result<BasicValueEnum<'ctx>, Error>, Error> {
        let struct_ty = types::to_llvm(type_, self)?.into_struct_type();
        let mut tuple = struct_ty.get_undef().into();
        for (i, component) in components.into_iter().enumerate() {
            let val = self.compile_expr(component)?;
            tuple = self
                .builder
                .build_insert_value(tuple, val, i as u32, "component")
                .unwrap();
        }
        Ok(Ok(tuple.into_struct_value().into()))
    }

    fn unpack_value(&mut self, target: &DefnTarget, value: BasicValueEnum<'ctx>) {
        match target {
            DefnTarget::Ignore => {}
            DefnTarget::Symbol(id) => {
                self.named_vals.insert(*id, value);
            }
            DefnTarget::Unpack(targets, _span) => {
                let value = value.into_struct_value();
                for (i, target) in targets.iter().enumerate() {
                    let component = self
                        .builder
                        .build_extract_value(value, i as u32, "component")
                        .unwrap();
                    self.unpack_value(target, component);
                }
            }
        }
    }
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

fn unit_value(c_ctx: &Context) -> BasicValueEnum<'_> {
    c_ctx.struct_type(&[], false).const_zero().into()
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
