use clap::Parser;

use crate::tools::add_solution_checks::AddSolutionChecksOpt;
use crate::tools::booklet::BookletOpt;
use crate::tools::clear::ClearOpt;
use crate::tools::copy_competition_files::CopyCompetitionFilesOpt;
use crate::tools::export_solution_checks::ExportSolutionChecksOpt;
use crate::tools::find_bad_case::FindBadCaseOpt;
use crate::tools::fuzz_checker::FuzzCheckerOpt;
use crate::tools::gen_autocompletion::GenAutocompletionOpt;
use crate::tools::reset::ResetOpt;
use crate::tools::sandbox::SandboxOpt;
use crate::tools::server::ServerOpt;
use crate::tools::task_info::TaskInfoOpt;
use crate::tools::terry_statement::TerryStatementOpt;
use crate::tools::worker::WorkerOpt;
use crate::LoggerOpt;

#[derive(Parser, Debug)]
#[clap(name = "task-maker-tools")]
pub struct Opt {
    #[clap(flatten, next_help_heading = Some("LOGGING"))]
    pub logger: LoggerOpt,

    /// Which tool to use
    #[clap(subcommand)]
    pub tool: Tool,
}

#[derive(Parser, Debug)]
pub enum Tool {
    /// Clear a task directory
    Clear(ClearOpt),
    /// Generate the autocompletion files for the shell
    GenAutocompletion(GenAutocompletionOpt),
    /// Spawn an instance of the server
    Server(ServerOpt),
    /// Spawn an instance of a worker
    Worker(WorkerOpt),
    /// Wipe the internal storage of task-maker
    ///
    /// Warning: no other instances of task-maker should be running when this flag is provided.
    Reset(ResetOpt),
    /// Run a command inside a sandbox similar to the one used by task-maker
    Sandbox(SandboxOpt),
    /// Obtain the information about a task.
    TaskInfo(TaskInfoOpt),
    /// Compile just the booklet for a task or a contest.
    Booklet(BookletOpt),
    /// Copy statements and attachments of a contest in a separate directory
    CopyCompetitionFiles(CopyCompetitionFilesOpt),
    /// Build terry statements by adding the subtask table
    TerryStatement(TerryStatementOpt),
    /// Fuzz the checker of a task.
    FuzzChecker(FuzzCheckerOpt),
    /// Generate and search for an input file that make a solution fail.
    FindBadCase(FindBadCaseOpt),
    /// Add the @check comments to the solutions.
    AddSolutionChecks(AddSolutionChecksOpt),
    /// Exports solution checks to json.
    ExportSolutionChecks(ExportSolutionChecksOpt),
    /// Run the sandbox instead of the normal task-maker.
    ///
    /// This option is left as undocumented as it's not part of the public API.
    #[clap(hide = true)]
    InternalSandbox,
}
