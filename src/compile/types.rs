use inkwell::{
    context::Context,
    types::{BasicType, BasicTypeEnum, FunctionType, IntType, PointerType, StructType},
};

use crate::{Error, Type, TypeRef};

use super::{CompileCtx, machine};

pub fn fn_to_llvm<'ctx>(
    type_: TypeRef,
    ctx: &CompileCtx<'ctx, '_>,
) -> Result<FunctionType<'ctx>, Error> {
    let Type::Function(params, ret) = ctx.types.get(type_)? else {
        panic!("expected function type")
    };
    let ret_ty = to_llvm(ret, ctx)?;
    let mut param_tys = vec![ptr(ctx.llvm).into()]; // first argument is captures
    for param in params {
        param_tys.push(to_llvm(param, ctx)?.into());
    }
    Ok(ret_ty.fn_type(&param_tys, false))
}

pub fn to_llvm<'ctx>(
    type_: TypeRef,
    ctx: &CompileCtx<'ctx, '_>,
) -> Result<BasicTypeEnum<'ctx>, Error> {
    match ctx.types.get(type_)? {
        Type::Function(_params, _ret) => {
            let addr = ptr(ctx.llvm).into();
            Ok(ctx.llvm.struct_type(&[addr, addr], false).into())
        }
        Type::Tuple(components) => {
            let comps = components
                .into_iter()
                .map(|comp| to_llvm(comp, ctx))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(ctx.llvm.struct_type(&comps, false).into())
        }
        Type::Array(_element) => Ok(array_ref(ctx.llvm).into()),
        Type::Bool => Ok(ctx.llvm.bool_type().into()),
        Type::Real => Ok(ctx.llvm.f32_type().into()),
        Type::Natural => Ok(ctx.llvm.i32_type().into()),
    }
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
