// These warnings show up in release mode, but they are not important
#![cfg_attr(not(debug_assertions), allow(dead_code, unused_macros, unused_imports))]

use std::time::SystemTime;

use regex::Regex;
use typescript_definitions::TypeScriptifyTrait;

use task_maker_dag::{ExecutionResourcesUsage, ExecutionResult, ExecutionStatus, File};
use task_maker_exec::{ClientInfo, ExecutorStatus, ExecutorWorkerStatus, WorkerCurrentJobStatus};
use task_maker_format::ioi::{
    BatchTypeData, Booklet, BookletConfig, Checker, CommunicationTypeData, IOITask, InputGenerator,
    InputValidator, OutputGenerator, Statement, StatementConfig, SubtaskInfo, TaskInfoScoring,
    TaskInfoStatement, TaskType, TestcaseInfo, TestcaseScoreAggregator,
};
use task_maker_format::ioi::{IOITaskInfo, TaskInfoAttachment, TaskInfoLimits, TaskInfoSubtask};
use task_maker_format::terry::TerryTaskInfo;
use task_maker_format::terry::{
    CaseStatus, SolutionAlert, SolutionFeedback, SolutionFeedbackCase, SolutionOutcome,
    SolutionValidation, SolutionValidationCase, TerryTask,
};
use task_maker_format::ui::{UIExecutionStatus, UIMessage};
use task_maker_format::TaskInfo;
use task_maker_lang::{Dependency, GraderMap, SourceFile};

/// Print to stdout the type definition for the specified type.
macro_rules! export_ts {
    ($t : ty) => {
        println!("{}", fix_bug_1(<$t>::type_script_ify()))
    };
}

/// Apply the fix from PR#1 fixing some invalid types.
///
/// https://github.com/arabidopsis/typescript-definitions/pull/1
fn fix_bug_1<S: AsRef<str>>(def: S) -> String {
    let regex = Regex::new(r"(?m)\[\s*(\w+)\s*:\s*(\w+)\s*\]").unwrap();
    let substitution = "[$1 in $2]";
    regex.replace_all(def.as_ref(), substitution).to_string()
}

#[cfg(debug_assertions)]
fn main() {
    println!("// Type aliases");
    println!("export type SubtaskId = number;");
    println!("export type TestcaseId = number;");
    println!("export type FileUuid = string;");
    println!("export type WorkerUuid = string;");
    println!("export type ClientUuid = string;");
    println!("export type Seed = number;");
    println!("export type Language = string;");
    println!("export type Mutex<T> = T;");
    export_ts!(UIMessage);
    export_ts!(UIExecutionStatus);
    export_ts!(ExecutorStatus<SystemTime>);
    export_ts!(ExecutorWorkerStatus<SystemTime>);
    export_ts!(WorkerCurrentJobStatus<SystemTime>);
    export_ts!(ClientInfo);
    export_ts!(IOITask);
    export_ts!(TerryTask);
    export_ts!(SolutionOutcome);
    export_ts!(ExecutionResult);
    export_ts!(TaskType);
    export_ts!(SubtaskInfo);
    export_ts!(TestcaseInfo);
    export_ts!(TestcaseScoreAggregator);
    export_ts!(GraderMap);
    export_ts!(Dependency);
    export_ts!(File);
    export_ts!(Booklet);
    export_ts!(BookletConfig);
    export_ts!(Statement);
    export_ts!(StatementConfig);
    export_ts!(SolutionValidation);
    export_ts!(SolutionValidationCase);
    export_ts!(SolutionAlert);
    export_ts!(CaseStatus);
    export_ts!(SolutionFeedback);
    export_ts!(SolutionFeedbackCase);
    export_ts!(ExecutionStatus);
    export_ts!(ExecutionResourcesUsage);
    export_ts!(BatchTypeData);
    export_ts!(CommunicationTypeData);
    export_ts!(Checker);
    export_ts!(SourceFile);
    export_ts!(InputGenerator);
    export_ts!(InputValidator);
    export_ts!(OutputGenerator);
    export_ts!(TaskInfo);
    export_ts!(IOITaskInfo);
    export_ts!(TaskInfoLimits);
    export_ts!(TaskInfoAttachment);
    export_ts!(TaskInfoSubtask);
    export_ts!(TaskInfoScoring);
    export_ts!(TaskInfoStatement);
    export_ts!(TerryTaskInfo);
}

#[cfg(not(debug_assertions))]
fn main() {
    panic!("This program should be compiled in debug mode");
}
