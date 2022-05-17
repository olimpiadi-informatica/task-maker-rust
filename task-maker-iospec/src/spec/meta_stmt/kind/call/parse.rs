use syn::parenthesized;
use syn::parse::*;
use syn::punctuated::Punctuated;
use syn::Token;

use crate::ast::*;

impl Parse for CallMetaStmt {
    fn parse(input: ParseStream) -> Result<Self> {
        let paren_input;
        Ok(Self {
            kw: input.parse()?,
            name: input.parse()?,
            paren: parenthesized!(paren_input in input),
            args: Punctuated::parse_terminated(&paren_input)?,
            ret: if input.peek(Token![->]) {
                Some(input.parse()?)
            } else {
                None
            },
            semi: input.parse()?,
        })
    }
}

impl Parse for CallArg {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            name: input.parse()?,
            eq: input.parse()?,
            kind: input.parse()?,
        })
    }
}

impl Parse for CallArgKind {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(if input.peek(Token![&]) {
            Self::Reference(input.parse()?)
        } else {
            Self::Value(input.parse()?)
        })
    }
}

impl Parse for CallByValueArg {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            expr: input.parse()?,
        })
    }
}

impl Parse for CallByReferenceArg {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            amp: input.parse()?,
            expr: input.parse()?,
        })
    }
}

impl Parse for CallRet {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            arrow: input.parse()?,
            kind: input.parse()?,
        })
    }
}

impl Parse for CallRetKind {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(if input.peek(syn::token::Paren) {
            Self::Tuple(input.parse()?)
        } else {
            Self::Single(input.parse()?)
        })
    }
}

impl Parse for SingleCallRet {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            expr: input.parse()?,
        })
    }
}

impl Parse for TupleCallRet {
    fn parse(input: ParseStream) -> Result<Self> {
        let paren_input;
        Ok(Self {
            paren: parenthesized!(paren_input in input),
            items: Punctuated::parse_separated_nonempty(&paren_input)?,
        })
    }
}
