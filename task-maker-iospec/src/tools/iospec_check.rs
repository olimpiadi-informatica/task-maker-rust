use std::fs::File;
use std::io;
use std::io::BufReader;
use std::path::PathBuf;

use anyhow::Error;
use clap::Parser;

use crate::run::Run;
use crate::*;

use super::share::SpecOpt;

#[derive(Parser, Debug, Clone)]
pub struct Opt {
    #[clap(flatten)]
    pub spec: SpecOpt,
    pub input: Option<PathBuf>,
    pub output: Option<PathBuf>,
}

pub fn do_main(opt: Opt, stderr: &mut dyn io::Write) -> Result<(), Error> {
    let (ir, dgns) = opt.spec.load(stderr, vec!["check".into()])?;

    match (opt.input, opt.output) {
        (Some(input), output) => ir
            .run(
                &mut Default::default(),
                &mut run::Context {
                    input_source: run::IoSource(Box::new(BufReader::new(
                        File::open(input).unwrap(),
                    ))),
                    output_source: output.map(|output| {
                        run::IoSource(Box::new(BufReader::new(File::open(output).unwrap())))
                    }),
                    dgns,
                },
            )
            .or_else(|stop| stop.as_result())?,
        _ => (),
    }

    Ok(())
}
