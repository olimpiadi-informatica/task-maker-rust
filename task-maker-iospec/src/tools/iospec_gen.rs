use std::fs::File;
use std::io;
use std::io::stdout;
use std::io::Write;
use std::path::PathBuf;

use anyhow::bail;
use anyhow::Error;
use clap::ArgEnum;
use clap::Parser;
use gen::gen_string;
use gen::Inspect;
use ir::Template;

use crate::lang::c::C;
use crate::lang::cpp::Cpp;
// use crate::lang::tex::Tex;
use crate::lang::cpp_lib::CppLib;
use crate::*;

use super::share::SpecOpt;

#[derive(Parser, Debug, Clone)]
pub struct Opt {
    #[clap(flatten)]
    pub spec: SpecOpt,
    #[clap(long, arg_enum)]
    pub lang: LangOpt,
    #[clap(long, arg_enum, default_value = "grader")]
    pub target: TargetOpt,
    #[clap(long)]
    pub dest: Option<PathBuf>,
}

#[derive(ArgEnum, Debug, Clone, Copy)]
pub enum LangOpt {
    C,
    Cpp,
    Inspect,
    // Tex,
}

#[derive(ArgEnum, Debug, Clone, Copy)]
pub enum TargetOpt {
    Grader,
    Template,
    Support,
}

pub fn do_main(opt: Opt, stderr: &mut dyn io::Write) -> Result<(), Error> {
    let (ir, _) = opt
        .spec
        .load(stderr, vec!["gen".into(), format!("lang={:?}", opt.lang)])?;

    let str = match (&opt.target, &opt.lang) {
        (TargetOpt::Grader, LangOpt::C) => gen_string(&ir, &C),
        (TargetOpt::Grader, LangOpt::Cpp) => gen_string(&ir, &Cpp),
        (TargetOpt::Grader, LangOpt::Inspect) => gen_string(&ir, &Inspect),
        (TargetOpt::Template, LangOpt::C) => gen_string(&Template(&ir), &C),
        (TargetOpt::Template, LangOpt::Cpp) => gen_string(&Template(&ir), &Cpp),
        (TargetOpt::Support, LangOpt::Cpp) => gen_string(&ir, &CppLib),
        _ => bail!(
            "unsupported combination: `--target {:?} --lang {:?}`",
            &opt.target,
            &opt.lang
        ),
    };

    match &opt.dest {
        Some(path) => File::create(path)?.write(str.as_bytes())?,
        None => stdout().write(str.as_bytes())?,
    };

    Ok(())
}
