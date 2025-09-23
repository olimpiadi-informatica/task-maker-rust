use clap::Parser;

use task_maker_rust::error::NiceError;
use task_maker_rust::tools::add_solution_checks::main_add_solution_checks;
use task_maker_rust::tools::booklet::main_booklet;
use task_maker_rust::tools::clear::main_clear;
use task_maker_rust::tools::copy_competition_files::copy_competition_files_main;
use task_maker_rust::tools::export_booklet::main_export_booklet;
use task_maker_rust::tools::export_solution_checks::main_export_solution_checks;
use task_maker_rust::tools::find_bad_case::main_find_bad_case;
use task_maker_rust::tools::fuzz_checker::main_fuzz_checker;
use task_maker_rust::tools::gen_autocompletion::main_get_autocompletion;
use task_maker_rust::tools::opt::{Opt, Tool};
use task_maker_rust::tools::reset::main_reset;
use task_maker_rust::tools::sandbox::main_sandbox;
use task_maker_rust::tools::server::main_server;
use task_maker_rust::tools::task_info::main_task_info;
use task_maker_rust::tools::terry_statement::main_terry_statement;
use task_maker_rust::tools::worker::main_worker;

fn main() {
    let base_opt = Opt::parse();
    base_opt.logger.enable_log();

    match base_opt.tool {
        Tool::Clear(opt) => main_clear(opt),
        Tool::GenAutocompletion(opt) => main_get_autocompletion(opt),
        Tool::Server(opt) => main_server(opt),
        Tool::Worker(opt) => main_worker(opt),
        Tool::Reset(opt) => main_reset(opt),
        Tool::Sandbox(opt) => main_sandbox(opt),
        Tool::TaskInfo(opt) => main_task_info(opt),
        Tool::Booklet(opt) => main_booklet(opt, base_opt.logger),
        Tool::TerryStatement(opt) => main_terry_statement(opt, base_opt.logger),
        Tool::CopyCompetitionFiles(opt) => copy_competition_files_main(opt, base_opt.logger),
        Tool::FuzzChecker(opt) => main_fuzz_checker(opt),
        Tool::FindBadCase(opt) => main_find_bad_case(opt),
        Tool::AddSolutionChecks(opt) => main_add_solution_checks(opt, base_opt.logger),
        Tool::ExportSolutionChecks(opt) => main_export_solution_checks(opt),
        Tool::ExportBooklet(opt) => main_export_booklet(opt),
        Tool::InternalSandbox => return task_maker_rust::main_sandbox(),
    }
    .nice_unwrap()
}
