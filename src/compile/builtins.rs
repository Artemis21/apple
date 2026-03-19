use inkwell::{
    FloatPredicate, IntPredicate,
    basic_block::BasicBlock,
    builder::Builder,
    module::Linkage,
    values::{BasicValue, BasicValueEnum, FunctionValue},
};

use crate::{Builtin, DefnId, TypeRef};

use super::{CompileCtx, types, unit_value};

pub fn compile_builtins<'ctx, 'obj>(
    builtins: &[(Builtin, DefnId)],
    ctx: &mut CompileCtx<'ctx, 'obj>,
) where
    'ctx: 'obj,
{
    for (builtin, defn) in builtins {
        let type_ = ctx.definitions.get_type(*defn).term; // TODO: polymorphism
        let val = compile_builtin(*builtin, type_, ctx);
        ctx.frames.last_mut().unwrap().locals.insert(*defn, val);
    }
}

fn compile_builtin<'ctx, 'obj>(
    builtin: Builtin,
    type_: TypeRef,
    ctx: &CompileCtx<'ctx, 'obj>,
) -> BasicValueEnum<'ctx>
where
    'ctx: 'obj,
{
    let fn_ty = types::fn_to_llvm(type_, ctx);
    let fn_val = ctx
        .module
        .add_function(builtin.name(), fn_ty, Some(Linkage::Private));
    let entry = ctx.llvm.append_basic_block(fn_val, "entry");
    ctx.builder.position_at_end(entry);
    match builtin {
        Builtin::Add => add(fn_val, ctx.builder),
        Builtin::Mul => mul(fn_val, ctx.builder),
        Builtin::Lt => lt(fn_val, ctx.builder),
        Builtin::Range => range(fn_val, entry, ctx),
        Builtin::Print => print(fn_val, ctx),
        Builtin::ToReal => to_real(fn_val, ctx),
        Builtin::Normal => normal(fn_val, ctx),
        _ => {
            ctx.builder.build_unreachable().unwrap(); // TODO: implement
        }
    }
    let fn_ptr = fn_val.as_global_value().as_pointer_value();
    let capture_ptr = types::ptr(ctx.llvm).const_null();
    ctx.llvm
        .const_struct(&[fn_ptr.into(), capture_ptr.into()], false)
        .into()
}

fn add(fn_val: FunctionValue<'_>, builder: &Builder<'_>) {
    let left = fn_val.get_nth_param(1).unwrap().into_float_value();
    let right = fn_val.get_nth_param(2).unwrap().into_float_value();
    let res = builder.build_float_add(left, right, "add").unwrap();
    builder.build_return(Some(&res)).unwrap();
}

fn mul(fn_val: FunctionValue<'_>, builder: &Builder<'_>) {
    let left = fn_val.get_nth_param(1).unwrap().into_int_value();
    let right = fn_val.get_nth_param(2).unwrap().into_int_value();
    let res = builder.build_int_mul(left, right, "mul").unwrap();
    builder.build_return(Some(&res)).unwrap();
}

fn lt(fn_val: FunctionValue<'_>, builder: &Builder<'_>) {
    let left = fn_val.get_nth_param(1).unwrap().into_float_value();
    let right = fn_val.get_nth_param(2).unwrap().into_float_value();
    let res = builder
        .build_float_compare(FloatPredicate::OLT, left, right, "lt")
        .unwrap();
    builder.build_return(Some(&res)).unwrap();
}

fn range(fn_val: FunctionValue<'_>, entry: BasicBlock<'_>, ctx: &CompileCtx<'_, '_>) {
    let head = ctx.llvm.insert_basic_block_after(entry, "loop_head");
    let body = ctx.llvm.insert_basic_block_after(head, "loop_body");
    let tail = ctx.llvm.insert_basic_block_after(head, "loop_tail");
    let nat_ty = ctx.llvm.i32_type();

    // entry: allocate array with length := max(low - high, 0)
    let low = fn_val.get_nth_param(1).unwrap().into_int_value();
    let high = fn_val.get_nth_param(2).unwrap().into_int_value();
    let diff = ctx.builder.build_int_sub(high, low, "diff").unwrap();
    let diff_ext = ctx
        .builder
        .build_int_z_extend(diff, types::isize(ctx.llvm), "diff_ext")
        .unwrap();
    let empty = ctx
        .builder
        .build_int_compare(IntPredicate::UGE, low, high, "empty")
        .unwrap();
    let zero = types::isize(ctx.llvm).const_zero();
    let len = ctx
        .builder
        .build_select(empty, zero, diff_ext, "len")
        .unwrap()
        .into_int_value();
    let array_ptr = ctx
        .builder
        .build_array_malloc(nat_ty, len, "heap array")
        .unwrap();
    ctx.builder.build_unconditional_branch(head).unwrap();

    // head: check if we're done, then go to body or tail
    ctx.builder.position_at_end(head);
    let idx_phi = ctx.builder.build_phi(nat_ty, "idx").unwrap();
    let idx = idx_phi.as_basic_value().into_int_value();
    let idx_ext = ctx
        .builder
        .build_int_z_extend(idx, types::isize(ctx.llvm), "idx_ext")
        .unwrap();
    let continue_ = ctx
        .builder
        .build_int_compare(IntPredicate::ULT, idx_ext, len, "continue")
        .unwrap();
    ctx.builder
        .build_conditional_branch(continue_, body, tail)
        .unwrap();

    // body: array[idx] := low + idx; idx' := idx + 1
    ctx.builder.position_at_end(body);
    let location = unsafe {
        ctx.builder
            .build_gep(nat_ty, array_ptr, &[idx], "elem loc")
            .unwrap()
    };
    let elem = ctx.builder.build_int_add(low, idx, "elem").unwrap();
    ctx.builder.build_store(location, elem).unwrap();
    let inc_idx = ctx
        .builder
        .build_int_add(idx, nat_ty.const_int(1, false), "inc idx")
        .unwrap();
    idx_phi.add_incoming(&[(&nat_ty.const_zero(), entry), (&inc_idx, body)]);
    ctx.builder.build_unconditional_branch(head).unwrap();

    // tail: return array as { ptr, len }
    ctx.builder.position_at_end(tail);
    ctx.builder
        .build_aggregate_return(&[array_ptr.into(), len.into()])
        .unwrap();
}

fn print<'ctx, 'obj>(fn_val: FunctionValue<'ctx>, ctx: &CompileCtx<'ctx, 'obj>)
where
    'ctx: 'obj,
{
    let printf_ty = ctx
        .llvm
        .i32_type()
        .fn_type(&[types::ptr(ctx.llvm).into()], true);
    let printf = ctx.module.add_function("printf", printf_ty, None);
    let val = fn_val.get_nth_param(1).unwrap().into_float_value();
    let format = ctx
        .builder
        .build_global_string_ptr("%f\n", "printf format")
        .unwrap()
        .as_basic_value_enum();
    let val_promote = ctx
        .builder
        .build_float_cast(val, ctx.llvm.f64_type(), "promote")
        .unwrap();
    ctx.builder
        .build_call(printf, &[format.into(), val_promote.into()], "print")
        .unwrap();
    ctx.builder
        .build_return(Some(&unit_value(ctx.llvm)))
        .unwrap();
}

fn to_real(fn_val: FunctionValue<'_>, ctx: &CompileCtx<'_, '_>) {
    let nat = fn_val.get_nth_param(1).unwrap().into_int_value();
    let res = ctx
        .builder
        .build_unsigned_int_to_float(nat, ctx.llvm.f32_type(), "to_real")
        .unwrap();
    ctx.builder.build_return(Some(&res)).unwrap();
}

fn normal(fn_val: FunctionValue<'_>, ctx: &CompileCtx<'_, '_>) {
    let rand_ty = ctx.llvm.i32_type().fn_type(&[], false);
    let rand = ctx.module.add_function("rand", rand_ty, None);
    let _mean = fn_val.get_nth_param(1).unwrap().into_float_value();
    let _stddev = fn_val.get_nth_param(2).unwrap().into_float_value();
    // TODO: actually normal dist.
    let rand_int = ctx
        .builder
        .build_call(rand, &[], "rand_int")
        .unwrap()
        .try_as_basic_value()
        .unwrap_basic();
    let res = ctx
        .builder
        .build_bit_cast(rand_int, ctx.llvm.f32_type(), "rand")
        .unwrap();
    ctx.builder.build_return(Some(&res)).unwrap();
}
