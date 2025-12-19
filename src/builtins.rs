use crate::{Environment, Type, TypeContext};

macro_rules! initial_env {
    { for< $( $quantified:ident ),* > $( $name:tt: $( $t:tt )->+ ,)* } => {
        pub fn initial_env(ctx: &mut TypeContext) -> Environment {
            $( let $quantified = ctx.fresh(); )*
            let mut env = Environment::default();
            $(
                env.assign_symbol(
                    stringify!($name).to_string(),
                    type_ref!([ctx] $( $t )->+ ),
                    ctx,
                );
            )*
            env
        }
    };
}

macro_rules! type_ref {
    ( [$ctx:ident] ( $( $( $param_t:tt )->+ ),* ) -> $( $ret_t:tt )->+ ) => {{
        let params = vec![ $( type_ref!( [$ctx] $( $param_t )->+ ) ),* ];
        let ret = type_ref!( [$ctx] $( $ret_t )->+ );
        $ctx.const_type(Type::Function(params, ret))
    }};
    ( [$ctx:ident] ( $( $( $t:tt )->+ ),* ) ) => {{
        let components = vec![ $( type_ref!( [$ctx] $( $t )->+ ) ),* ];
        $ctx.const_type(Type::Tuple(components))
    }};
    ( [$ctx:ident] [ $( $t:tt )->+ ] ) => {{
        let element = type_ref!( [$ctx] $( $t )->+ );
        $ctx.const_type(Type::Array(element))
    }};
    ( [$ctx:ident] $name:ident ) => {
        $ctx.const_type(Type::$name)
    };
    ( [$ctx:ident] { $name:ident } ) => {
        $name
    }
}

// TODO: typeclass polymorphism
initial_env! {
    for <t0>
    normal: (Real, Real) -> Real,
    ..: (Natural, Natural) -> [Natural],
    sum: ([Real]) -> Real,
    load: () -> [Real],
    print: ({t0}) -> (),
    to_real: (Natural) -> Real,
    @: ([Real], [Real]) -> Real,
    +: (Real, Real) -> Real,
    -: (Real, Real) -> Real,
    *: (Real, Real) -> Real,
    <: (Real, Real) -> Bool,
}
