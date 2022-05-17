use task_maker_iospec::tools::iospec_check::*;

fn main() -> Result<(), anyhow::Error> {
    return do_main(clap::Parser::parse(), &mut std::io::stderr());
}
