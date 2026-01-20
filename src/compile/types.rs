use inkwell::{
    context::Context,
    types::{BasicType, BasicTypeEnum, FunctionType, IntType, PointerType, StructType},
};

use crate::{Type, TypeRef};

use super::{CompileCtx, machine};

pub fn fn_to_llvm<'ctx>(type_: TypeRef, ctx: &CompileCtx<'ctx, '_>) -> FunctionType<'ctx> {
    let Type::Function(params, ret) = ctx.types.get(type_).expect("unbound type variable") else {
        panic!("expected function type")
    };
    let ret_ty = to_llvm(ret, ctx);
    let mut param_tys = vec![ptr(ctx.llvm).into()]; // first argument is captures
    for param in params {
        param_tys.push(to_llvm(param, ctx).into());
    }
    ret_ty.fn_type(&param_tys, false)
}

pub fn to_llvm<'ctx>(type_: TypeRef, ctx: &CompileCtx<'ctx, '_>) -> BasicTypeEnum<'ctx> {
    match ctx.types.get(type_).expect("unbound type variable") {
        Type::Function(_params, _ret) => func_ref(ctx.llvm).into(),
        Type::Tuple(components) => {
            let comps = components
                .into_iter()
                .map(|comp| to_llvm(comp, ctx))
                .collect::<Vec<_>>();
            ctx.llvm.struct_type(&comps, false).into()
        }
        Type::Array(_element) => array_ref(ctx.llvm).into(),
        Type::Bool => ctx.llvm.bool_type().into(),
        Type::Real => ctx.llvm.f32_type().into(),
        Type::Natural => ctx.llvm.i32_type().into(),
    }
}

pub fn func_ref(llvm: &Context) -> StructType<'_> {
    let addr = ptr(llvm).into();
    llvm.struct_type(&[addr, addr], false)
}

pub fn array_ref(llvm: &Context) -> StructType<'_> {
    let addr = ptr(llvm).into();
    let len = isize(llvm).into();
    llvm.struct_type(&[addr, len], false)
}

pub fn isize(llvm: &Context) -> IntType<'_> {
    llvm.ptr_sized_int_type(&machine().get_target_data(), Some(0.into()))
}

pub fn ptr(llvm: &Context) -> PointerType<'_> {
    llvm.ptr_type(0.into())
}
