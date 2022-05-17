use crate::ast;
use crate::compile::*;
use crate::ir::*;

impl CompileFrom<ast::CallMetaStmt> for CallMetaStmt {
    fn compile(ast: &ast::CallMetaStmt, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
        let ast::CallMetaStmt {
            kw,
            name,
            paren,
            args,
            ret,
            semi,
        } = ast;

        let (args, arg_commas) = unzip_punctuated(args.clone());

        Ok(Self {
            kw: kw.clone(),
            name: name.compile(env, dgns)?,
            paren: paren.clone(),
            args: args
                .iter()
                .map(|a| a.compile(env, dgns))
                .collect::<Result<_>>()?,
            arg_commas,
            ret: CallRet(ret.as_ref().map(|ret| ret.compile(env, dgns)).transpose()?),
            semi: semi.clone(),
        })
    }
}

impl CompileFrom<ast::CallArg> for CallArg {
    fn compile(ast: &ast::CallArg, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
        let ast::CallArg { name, eq, kind } = ast;

        Ok(Self {
            name: name.compile(env, dgns)?,
            eq: eq.clone(),
            kind: kind.compile(env, dgns)?,
        })
    }
}

impl CompileFrom<ast::CallArgKind> for CallArgKind {
    fn compile(ast: &ast::CallArgKind, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
        Ok(match ast {
            ast::CallArgKind::Value(arg) => CallArgKind::Value(arg.compile(env, dgns)?),
            ast::CallArgKind::Reference(arg) => CallArgKind::Reference(arg.compile(env, dgns)?),
        })
    }
}

impl CompileFrom<ast::CallByValueArg> for CallByValueArg {
    fn compile(ast: &ast::CallByValueArg, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
        let ast::CallByValueArg { expr } = ast;
        Ok(Self {
            expr: expr.compile(env, dgns)?,
        })
    }
}

impl CompileFrom<ast::CallByReferenceArg> for CallByReferenceArg {
    fn compile(
        ast: &ast::CallByReferenceArg,
        env: &Env,
        dgns: &mut DiagnosticContext,
    ) -> Result<Self> {
        let ast::CallByReferenceArg { amp, expr } = ast;

        Ok(Self {
            amp: amp.clone(),
            expr: expr.compile(env, dgns)?,
        })
    }
}

impl CompileFrom<ast::CallRet> for CallRetExpr {
    fn compile(ast: &ast::CallRet, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
        let ast::CallRet { arrow, kind } = ast;

        Ok(Self {
            arrow: arrow.clone(),
            kind: kind.compile(env, dgns)?,
        })
    }
}

impl CompileFrom<ast::CallRetKind> for CallRetKind {
    fn compile(ast: &ast::CallRetKind, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
        Ok(match ast {
            ast::CallRetKind::Single(ret) => Self::Single(ret.compile(env, dgns)?),
            ast::CallRetKind::Tuple(ret) => Self::Tuple(ret.compile(env, dgns)?),
        })
    }
}

impl CompileFrom<ast::SingleCallRet> for SingleCallRet {
    fn compile(ast: &ast::SingleCallRet, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
        let ast::SingleCallRet { expr } = ast;

        Ok(Self {
            expr: expr.compile(env, dgns)?,
        })
    }
}

impl CompileFrom<ast::TupleCallRet> for TupleCallRet {
    fn compile(ast: &ast::TupleCallRet, env: &Env, dgns: &mut DiagnosticContext) -> Result<Self> {
        let ast::TupleCallRet { items, paren } = ast;

        let (items, item_commas) = unzip_punctuated(items.clone());

        Ok(Self {
            paren: paren.clone(),
            items: items
                .iter()
                .map(|item| item.compile(env, dgns))
                .collect::<Result<_>>()?,
            item_commas,
        })
    }
}
