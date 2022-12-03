#![allow(dead_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use task_maker_dag::{ExecutionResourcesUsage, ExecutionResult, ExecutionStatus};
use task_maker_format::ioi::sanity_checks::get_sanity_checks;
use task_maker_format::ioi::*;
use task_maker_lang::GraderMap;

pub fn new_task() -> IOITask {
    new_task_with_context(Path::new(""))
}

pub fn new_task_with_context(path: &Path) -> IOITask {
    let p = path.join("x");
    if path.as_os_str() != "" {
        std::fs::write(&p, "xxx").unwrap();
    }
    let mut task = IOITask {
        path: path.into(),
        task_type: TaskType::Batch(BatchTypeData {
            output_generator: None,
            checker: Checker::WhiteDiff,
        }),
        name: "task".to_string(),
        title: "The Task".to_string(),
        time_limit: None,
        memory_limit: None,
        infile: None,
        outfile: None,
        subtasks: HashMap::new(),
        input_validator_generator: Default::default(),
        testcase_score_aggregator: TestcaseScoreAggregator::Min,
        grader_map: Arc::new(GraderMap::new(Vec::<PathBuf>::new())),
        booklets: vec![],
        difficulty: None,
        syllabus_level: None,
        sanity_checks: Arc::new(get_sanity_checks(&[])),
    };
    let st0 = task.subtasks.entry(0).or_insert(SubtaskInfo {
        id: 0,
        name: None,
        description: None,
        max_score: 10.0,
        testcases: HashMap::default(),
        span: None,
    });
    st0.testcases.entry(0).or_insert_with(|| {
        TestcaseInfo::new(
            0,
            InputGenerator::StaticFile(p.clone()),
            InputValidator::AssumeValid,
            OutputGenerator::StaticFile(p.clone()),
        )
    });
    let st1 = task.subtasks.entry(1).or_insert(SubtaskInfo {
        id: 1,
        name: None,
        description: None,
        max_score: 90.0,
        testcases: HashMap::default(),
        span: None,
    });
    st1.testcases.entry(1).or_insert_with(|| {
        TestcaseInfo::new(
            1,
            InputGenerator::StaticFile(p.clone()),
            InputValidator::AssumeValid,
            OutputGenerator::StaticFile(p.clone()),
        )
    });
    st1.testcases.entry(2).or_insert_with(|| {
        TestcaseInfo::new(
            2,
            InputGenerator::StaticFile(p.clone()),
            InputValidator::AssumeValid,
            OutputGenerator::StaticFile(p),
        )
    });
    task
}

pub fn good_result() -> ExecutionResult {
    ExecutionResult {
        status: ExecutionStatus::Success,
        was_killed: false,
        was_cached: false,
        resources: ExecutionResourcesUsage {
            cpu_time: 0.0,
            sys_time: 0.0,
            wall_time: 0.0,
            memory: 0,
        },
        stdout: None,
        stderr: None,
    }
}

pub fn bad_result() -> ExecutionResult {
    ExecutionResult {
        status: ExecutionStatus::ReturnCode(123),
        was_killed: false,
        was_cached: false,
        resources: ExecutionResourcesUsage {
            cpu_time: 0.0,
            sys_time: 0.0,
            wall_time: 0.0,
            memory: 0,
        },
        stdout: None,
        stderr: None,
    }
}
