use anyhow::Error;
use clap::Parser;
use std::io;

use super::iospec_gen::LangOpt;
use super::iospec_gen::TargetOpt;
use super::iospec_gen::{self};
use super::share::SpecOpt;

#[derive(Parser, Debug, Clone)]
pub struct Opt {
    #[clap(flatten)]
    pub spec: SpecOpt,
}

pub fn do_main(opt: Opt, stderr: &mut dyn io::Write) -> Result<(), Error> {
    let gen_targets = vec![
        (
            "gen/iospec.hpp",
            LangOpt::Cpp,
            TargetOpt::Support,
            vec!["support"],
        ),
        (
            "sol/grader.cpp",
            LangOpt::Cpp,
            TargetOpt::Parser,
            vec!["grader"],
        ),
        (
            "sol/grader.c",
            LangOpt::C,
            TargetOpt::Parser,
            vec!["grader"],
        ),
        (
            "sol/template.cpp",
            LangOpt::Cpp,
            TargetOpt::Template,
            vec!["template"],
        ),
        (
            "sol/template.c",
            LangOpt::C,
            TargetOpt::Template,
            vec!["template"],
        ),
    ];

    let copy_targets = vec![
        ("gen/iolib.hpp", include_str!("../assets/iolib.hpp")),
        (
            "gen/sample.generator.cpp",
            include_str!("../assets/sample.generator.cpp"),
        ),
        (
            "gen/sample.validator.cpp",
            include_str!("../assets/sample.validator.cpp"),
        ),
        (
            "gen/sample.checker.cpp",
            include_str!("../assets/sample.checker.cpp"),
        ),
        ("gen/IOSPEC.sample", include_str!("../assets/IOSPEC.sample")),
    ];

    for (path, lang, target, cfg) in gen_targets.into_iter() {
        let SpecOpt {
            spec,
            cfg: base_cfg,
            color,
        } = opt.spec.clone();
        let extra_cfg: Vec<_> = cfg.iter().map(|s| s.to_string()).collect();

        eprintln!("Generating `{}`...", path);

        iospec_gen::do_main(
            iospec_gen::Opt {
                spec: SpecOpt {
                    spec,
                    cfg: base_cfg.into_iter().chain(extra_cfg.into_iter()).collect(),
                    color,
                },
                target,
                lang,
                dest: Some(path.into()),
            },
            stderr,
        )
        .unwrap_or_else(|error| eprintln!("error while generating {}: {}", path, error));
    }

    for (path, content) in copy_targets.into_iter() {
        eprintln!("Adding `{}`...", path);

        std::fs::write(path, content)
            .unwrap_or_else(|error| eprintln!("error while generating {}: {}", path, error));
    }

    Ok(())
}
