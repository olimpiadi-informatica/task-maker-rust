macro_rules! gen {
    ($ctx:expr) => {
        {
            {
                // Pretend to use the arg
                let _ctx = &$ctx;
            }
            Ok(())
        }
    };
    ($ctx:expr, ) => {
        {
            gen!($ctx)
        }
    };
    ($ctx:expr, ; $($rest:tt)* ) => {
        {
            $ctx.endl()?;
            gen!($ctx, $($rest)*)
        }
    };
    ($ctx:expr, $lit:literal % ( $($expr:expr),* $(,)? ) $( ; $($rest:tt)* )?) => {
        {
            let GenBuffer { lang, indent, .. } = *$ctx;
            $ctx.append(format_args!($lit, $( GenToken { lang, indent, inner: $expr } ),*))?;
            gen!($ctx, $( ; $($rest)* )?)
        }
    };
    ($ctx:expr, $lit:literal % $expr:expr $( ; $($rest:tt)* )?) => {
        {
            gen!($ctx, $lit % ($expr))?;
            gen!($ctx, $( ; $($rest)* )?)
        }
    };
    ($ctx:expr, $lit:literal $( ; $($rest:tt)* )?) => {
        {
            $ctx.append($lit)?;
            gen!($ctx, $( ; $($rest)* )?)
        }
    };
    ($ctx:expr, $ident:ident $( ; $($rest:tt)* )?) => {
        {
            gen!($ctx, ( $ident ) $( ; $($rest)* )?)
        }
    };
    ($ctx:expr, || $body:expr $( ; $($rest:tt)* )?) => {
        {
            {
                let _: Result = $body;
            }
            gen!($ctx, $( $($rest)* )?)
        }
    };
    ($ctx:expr, ({ $($body:tt)* }) ; $($rest:tt)*) => {
        {
            $ctx.block_begin()?;
            gen!($ctx, $($body)*)?;
            $ctx.block_end()?;
            gen!($ctx, $($rest)*)
        }
    };
    ($ctx:expr, () $( ; $($rest:tt)* )?) => {
        {
            gen!($ctx, $( ; $($rest)* )?)
        }
    };
    ($ctx:expr, ( $( $expr:expr ),* ) $( ; $($rest:tt)* )?) => {
        {
            $(
                $ctx.gen($expr)?;
            )*
            gen!($ctx, $( $($rest)* )?)
        }
    };
    ($ctx:expr, { $($body:tt)* } $($rest:tt)*) => {
        {
            gen!($ctx, $($body)*)?;
            gen!($ctx, $($rest)*)
        }
    };
}

pub(crate) use gen;
