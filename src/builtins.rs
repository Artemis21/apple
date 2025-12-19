use crate::{Environment, Type, TypeContext};

macro_rules! initial_env {
    { $( $name:tt: $( $t:tt )->+ ,)* } => {
        pub fn initial_env(ctx: &mut TypeContext) -> Environment {
            let mut env = Environment::default();
            $(
                env.assign_symbol(stringify!($name).to_string(), type_ref!([ctx] $( $t )->+ ));
            )*
            env
        }
    };
}

macro_rules! type_ref {
    ( [$ctx:ident] ( $( $( $arg_t:tt )->+ ),* ) -> $( $ret_t:tt )->+ ) => {{
        let params = vec![ $( type_ref!( [$ctx] $( $arg_t )->+ ) ),* ];
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
}

// TODO: polymorphism
initial_env!{
    normal: (Real, Real) -> Real,
    ..: (Natural, Natural) -> [Natural],
    sum: ([Real]) -> Real,
    load: () -> [Real],
    print: ([Real]) -> (),
    to_real: (Natural) -> Real,
    @: ([Real], [Real]) -> Real,
    +: (Real, Real) -> Real,
    -: (Real, Real) -> Real,
    *: (Real, Real) -> Real,
    <: (Real, Real) -> Bool,
}
