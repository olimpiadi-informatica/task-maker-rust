use std::fmt::Display;
use std::fmt::Write;

mod gen_macro;
pub(crate) use gen_macro::*;

pub struct Inspect;

pub struct CommonMixin<'a, L>(pub &'a L);

macro_rules! lang_mixin {
    ($lang_ty:ty, $target:ty, $mixin_ty:expr) => {
        impl Gen<$lang_ty> for $target {
            fn gen(&self, ctx: GenContext<$lang_ty>) -> Result {
                let mixin = $mixin_ty(ctx.lang);
                let ctx = &mut ctx.with_lang(&mixin);
                self.gen(ctx)
            }
        }
    };
}

pub(crate) use lang_mixin;

macro_rules! lang_same_as {
    ($lang_ty:ty, $target:ty, $other_lang:expr) => {
        impl Gen<$lang_ty> for $target {
            fn gen(&self, ctx: GenContext<$lang_ty>) -> Result {
                let mut ctx2 = ctx.with_lang(&$other_lang);
                self.gen(&mut ctx2)
            }
        }
    };
}

pub(crate) use lang_same_as;

pub use std::fmt::Result;

pub type GenContext<'a, 'b, L> = &'a mut GenBuffer<'b, L>;

pub struct GenBuffer<'a, L> {
    pub lang: &'a L,
    pub fmt: &'a mut dyn Write,
    pub needs_indent: &'a mut bool,
    pub indent: u8,
}

pub struct GenOutput<W: std::io::Write>(W);

impl<W: std::io::Write> Write for GenOutput<W> {
    fn write_str(&mut self, s: &str) -> Result {
        self.0.write_all(s.as_bytes()).map_err(|_| std::fmt::Error)
    }
}

pub trait Gen<L> {
    fn gen(&self, ctx: GenContext<L>) -> Result;
}

pub struct GenToken<'a, L, T> {
    pub lang: &'a L,
    pub inner: &'a T,
    pub indent: u8,
}

impl<L, T: Gen<L>> Display for GenToken<'_, L, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result {
        let Self {
            indent,
            lang,
            inner,
        } = self;
        inner.gen(&mut GenBuffer {
            lang,
            fmt: f,
            needs_indent: &mut false,
            indent: *indent,
        })
    }
}

const INDENT_ONE: &'static str = "    ";

impl<L> GenBuffer<'_, L> {
    pub fn append<T: Display>(&mut self, token: T) -> Result {
        if *self.needs_indent {
            for _ in 0..self.indent {
                write!(self.fmt, "{}", INDENT_ONE)?;
            }
            *self.needs_indent = false;
        }
        write!(self.fmt, "{}", token)
    }

    pub fn gen<T: Gen<L>>(&mut self, token: &T) -> Result {
        token.gen(self)
    }

    pub fn endl(&mut self) -> Result {
        writeln!(self.fmt, "")?;
        *self.needs_indent = true;
        Ok(())
    }

    pub fn indent(&mut self) {
        self.indent += 1;
    }

    pub fn dedent(&mut self) {
        self.indent -= 1;
    }

    pub fn block_begin(&mut self) -> Result {
        self.indent();
        Ok(())
    }

    pub fn block_end(&mut self) -> Result {
        self.dedent();
        Ok(())
    }

    pub fn with_lang<'b, M>(&'b mut self, lang: &'b M) -> GenBuffer<'b, M> {
        return GenBuffer {
            lang,
            fmt: self.fmt,
            needs_indent: self.needs_indent,
            indent: self.indent,
        };
    }
}

pub fn gen_string<L, T: Gen<L>>(item: &T, lang: &L) -> String {
    let mut str = String::new();

    let mut ctx = GenBuffer {
        lang,
        fmt: &mut str,
        indent: 0,
        needs_indent: &mut true,
    };

    item.gen(&mut ctx).unwrap();

    str
}

#[cfg(test)]
mod test {
    use super::*;

    pub struct MyLang;

    impl Gen<MyLang> for i32 {
        fn gen(&self, ctx: &mut GenBuffer<MyLang>) -> Result {
            ctx.append(self)
        }
    }

    #[test]
    fn a() {
        assert_eq!("1", gen_string(&1, &MyLang))
    }
}

impl<L, T> Gen<L> for crate::ir::Ir<T>
where
    T: Gen<L>,
{
    fn gen(&self, ctx: GenContext<L>) -> Result {
        self.as_ref().gen(ctx)
    }
}

pub struct Punctuated<T, P>(pub Vec<T>, pub P);

impl<L, T, P> Gen<L> for Punctuated<T, P>
where
    T: Gen<L>,
    P: Display,
{
    fn gen(&self, ctx: GenContext<L>) -> Result {
        let mut first = true;
        let Self(items, punct) = self;

        for item in items {
            if !first {
                {
                    ctx.append(&punct)?;
                }
            }
            first = false;
            gen!(ctx, item)?;
        }

        gen!(ctx)
    }
}

/// Newtype for items that are generated verbatim
pub struct Raw<T>(pub T);

impl<L, T> Gen<L> for Raw<T>
where
    T: Display,
{
    fn gen(&self, ctx: GenContext<L>) -> Result {
        ctx.append(&self.0)
    }
}

impl<L> Gen<L> for () {
    fn gen(&self, ctx: GenContext<L>) -> Result {
        gen!(ctx)
    }
}
