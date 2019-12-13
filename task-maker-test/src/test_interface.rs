use itertools::Itertools;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use structopt::StructOpt;
use task_maker_dag::ExecutionStatus;
use task_maker_format::ioi::{
    CompilationStatus, SolutionEvaluationState, SubtaskId, Task, TestcaseEvaluationStatus,
    TestcaseGenerationStatus, UIState,
};
use task_maker_format::EvaluationConfig;
use task_maker_rust::{main_server, main_worker, run_evaluation, Evaluation, Opt, Remote};
use tempdir::TempDir;

/// Interface for testing a task.
#[derive(Debug)]
pub struct TestInterface {
    /// The path to the task directory.
    pub path: PathBuf,
    /// The time limit of the task.
    pub time_limit: Option<f64>,
    /// The memory limit of the task.
    pub memory_limit: Option<u64>,
    /// The maximum score of the task.
    pub max_score: Option<f64>,
    /// The list of the names of the files that must compile.
    pub must_compile: Vec<PathBuf>,
    /// The list of the names of the files that must fail to compile.
    pub must_not_compile: Vec<PathBuf>,
    /// The list of the names of the files that should not be compiled.
    pub not_compiled: Vec<PathBuf>,
    /// The list of the scores of the subtasks.
    pub subtask_scores: Option<Vec<f64>>,
    /// The list of scores, for each subtask, of the solutions.
    pub solution_scores: HashMap<PathBuf, Vec<f64>>,
    /// The status of the evaluation of some solutions.
    pub solution_statuses: HashMap<PathBuf, Vec<TestcaseEvaluationStatus>>,
    /// Expect task-maker to fail with the specified message.
    pub fail: Option<String>,
    /// The status of the generations of the testcases.
    pub generation_statuses: Option<Vec<TestcaseGenerationStatus>>,
    /// A list with the stderr message of the failing generators.
    pub generation_fails: Option<Vec<Option<String>>>,
    /// A list with the stderr message of the failing validations.
    pub validation_fails: Option<Vec<Option<String>>>,
    /// Whether the cache is allowed.
    pub cache: bool,
    /// Storage directory for the test interface. This is used only by the client, not the server
    /// nor the workers.
    pub store_dir: TempDir,
}

impl TestInterface {
    /// Make a new `TestInterface` from the specified task directory.
    pub fn new<P: Into<PathBuf>>(path: P) -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tasks")
            .join(path.into());
        TestInterface {
            path,
            time_limit: None,
            memory_limit: None,
            max_score: None,
            must_compile: Vec::new(),
            must_not_compile: Vec::new(),
            not_compiled: Vec::new(),
            subtask_scores: None,
            solution_scores: HashMap::new(),
            solution_statuses: HashMap::new(),
            fail: None,
            generation_statuses: None,
            generation_fails: None,
            validation_fails: None,
            cache: false,
            store_dir: TempDir::new("tm-test").unwrap(),
        }
    }

    /// Check that task-maker fails with the specified message.
    pub fn fail<S: Into<String>>(&mut self, message: S) -> &mut Self {
        self.fail = Some(message.into());
        self
    }

    /// Check that the time limit is the one specified.
    pub fn time_limit(&mut self, time_limit: f64) -> &mut Self {
        self.time_limit = Some(time_limit);
        self
    }

    /// Check that the memory limit is the one specified.
    pub fn memory_limit(&mut self, memory_limit: u64) -> &mut Self {
        self.memory_limit = Some(memory_limit);
        self
    }

    /// Check that the max score of the task is the one specified.
    pub fn max_score(&mut self, max_score: f64) -> &mut Self {
        self.max_score = Some(max_score);
        self
    }

    /// Check that the specified file is compiled successfully.
    pub fn must_compile<P: Into<PathBuf>>(&mut self, source: P) -> &mut Self {
        self.must_compile.push(source.into());
        self
    }

    /// Check that the specified file fails to compile.
    pub fn must_not_compile<P: Into<PathBuf>>(&mut self, source: P) -> &mut Self {
        self.must_not_compile.push(source.into());
        self
    }

    /// Check that the specified file is not compiled.
    pub fn not_compiled<P: Into<PathBuf>>(&mut self, source: P) -> &mut Self {
        self.not_compiled.push(source.into());
        self
    }

    /// Check that the subtasks have the following scores.
    pub fn subtask_scores<I: IntoIterator<Item = f64>>(&mut self, scores: I) -> &mut Self {
        self.subtask_scores = Some(scores.into_iter().collect());
        self
    }

    /// Check that the solution scores those values for each subtask.
    pub fn solution_score<P: Into<PathBuf>, I: IntoIterator<Item = f64>>(
        &mut self,
        solution: P,
        scores: I,
    ) -> &mut Self {
        self.solution_scores
            .entry(solution.into())
            .or_insert(scores.into_iter().collect());
        self
    }

    /// Check that the statuses of the solution starts with the ones specified.
    pub fn solution_statuses<P: Into<PathBuf>, I: IntoIterator<Item = TestcaseEvaluationStatus>>(
        &mut self,
        solution: P,
        statuses: I,
    ) -> &mut Self {
        self.solution_statuses
            .entry(solution.into())
            .or_insert(statuses.into_iter().collect());
        self
    }

    /// Check that the statuses of the generation are those.
    pub fn generation_statuses<I: IntoIterator<Item = TestcaseGenerationStatus>>(
        &mut self,
        statuses: I,
    ) -> &mut Self {
        self.generation_statuses = Some(statuses.into_iter().collect());
        self
    }

    /// Check that the generators fail with the specified messages.
    pub fn generation_fails<I: IntoIterator<Item = Option<String>>>(
        &mut self,
        fails: I,
    ) -> &mut Self {
        self.generation_fails = Some(fails.into_iter().collect());
        self
    }

    /// Check that the validations fail with the specified messages.
    pub fn validation_fails<I: IntoIterator<Item = Option<String>>>(
        &mut self,
        fails: I,
    ) -> &mut Self {
        self.validation_fails = Some(fails.into_iter().collect());
        self
    }

    /// Allow or disallow the cache.
    pub fn cache(&mut self, cache: bool) -> &mut Self {
        self.cache = cache;
        self
    }

    /// Run the tests using task-maker in local mode, i.e. not spawning a separate server and
    /// workers.
    pub fn run_local(&self) {
        self.run_task_maker(&[]);
    }

    /// Evaluate the task using a "remote" setup (spawning a local server and local workers).
    pub fn run_remote(&self) {
        if !port_scanner::scan_port(27182) {
            eprintln!("Server not spawned, spawning");
            TestInterface::spawn_server();
            TestInterface::wait_port(27182);
        }

        self.run_task_maker(&["--evaluate-on", "127.0.0.1:27182"]);
    }

    fn run_task_maker(&self, extra_args: &[&str]) {
        let mut args: Vec<&str> = vec!["task-maker"];
        let path = self.path.to_string_lossy().into_owned();
        let path = format!("--task-dir={}", path);
        args.push(&path);
        args.push("--ui=silent".into());
        if !self.cache {
            args.push("--no-cache".into());
        }
        args.push("--dry-run".into());
        args.push("-vv".into());
        let store_dir = format!("--store-dir={}", self.store_dir.path().to_string_lossy());
        args.push(&store_dir);
        for arg in extra_args {
            args.push(arg);
        }
        std::env::set_var(
            "TASK_MAKER_SANDBOX_BIN",
            PathBuf::from(env!("OUT_DIR")).join("sandbox"),
        );
        let opt = Opt::from_iter(&args);

        println!("Expecting: {:#?}", self);
        let task = Task::new(
            &self.path,
            &EvaluationConfig {
                solution_filter: vec![],
                booklet_solutions: false,
                no_statement: false,
                solution_paths: vec![],
            },
        )
        .unwrap();
        let state = Arc::new(Mutex::new(UIState::new(&task)));

        let state2 = state.clone();
        let res = run_evaluation(opt, move |_, mex| state2.lock().unwrap().apply(mex));
        match res {
            Ok(Evaluation::Done) => {
                if let Some(message) = &self.fail {
                    panic!(
                        "Expecting task-maker to fail with \"{}\" but didn't",
                        message
                    );
                }
            }
            Ok(Evaluation::Clean) => {
                panic!("Unexpected task cleaning");
            }
            Err(e) => {
                if let Some(message) = &self.fail {
                    if !e.to_string().contains(message) {
                        panic!(
                            "Expecting task-maker to fail with \"{}\" but failed with: {:?}",
                            message, e
                        );
                    } else {
                        return;
                    }
                } else {
                    panic!("Task-maker failed unexpectedly with: {:?}", e);
                }
            }
        }

        let state = state.lock().unwrap();
        println!("State is: {:#?}", state);
        self.check_limits(&state);
        self.check_compilation(&state);
        self.check_subtasks(&state);
        self.check_generations(&state);
        self.check_solution_scores(&state);
        self.check_solution_statuses(&state);
    }

    fn wait_port(port: u16) {
        for _ in 0..10 {
            eprintln!("Waiting for the server...");
            std::thread::sleep(Duration::from_millis(500));
            if port_scanner::scan_port(port) {
                break;
            }
        }
    }

    fn spawn_server() {
        std::thread::Builder::new()
            .name("Test server".to_string())
            .spawn(|| {
                let store = tempdir::TempDir::new("tm-test").unwrap();
                let store = store.path().to_string_lossy().to_string();
                let opt = Opt::from_iter(&[
                    "task-maker",
                    "--store-dir",
                    &store,
                    "--server",
                    "0.0.0.0:27182",
                    "0.0.0.0:27183",
                ]);
                if let Remote::Server(server_opt) = &opt.remote.as_ref().unwrap() {
                    let server_opt = server_opt.clone();
                    main_server(opt, server_opt);
                }
            })
            .unwrap();
        std::thread::Builder::new()
            .name("Test worker".to_string())
            .spawn(|| {
                TestInterface::wait_port(27183);
                let store = tempdir::TempDir::new("tm-test").unwrap();
                let store = store.path().to_string_lossy().to_string();
                let opt = Opt::from_iter(&[
                    "task-maker",
                    "--store-dir",
                    &store,
                    "--worker",
                    "127.0.0.1:27183",
                ]);
                if let Remote::Worker(worker_opt) = &opt.remote.as_ref().unwrap() {
                    let worker_opt = worker_opt.clone();
                    main_worker(opt, worker_opt);
                }
            })
            .unwrap();
    }

    /// Check the task limits are met.
    fn check_limits(&self, state: &UIState) {
        if let (Some(expected), Some(actual)) = (self.time_limit, state.task.time_limit) {
            assert!(abs_diff_eq!(expected, actual), "Wrong time limit");
        }
        if let (Some(expected), Some(actual)) = (self.memory_limit, state.task.memory_limit) {
            assert_eq!(expected, actual, "Wrong memory limit");
        }
        if let Some(max_score) = self.max_score {
            assert!(abs_diff_eq!(max_score, state.max_score), "Wrong max score");
        }
    }

    /// Check that the compilation of the files is good.
    fn check_compilation(&self, state: &UIState) {
        let compilations: HashMap<PathBuf, &CompilationStatus> = state
            .compilations
            .iter()
            .map(|(file, comp)| (PathBuf::from(file.file_name().unwrap()), comp))
            .collect();
        for name in self.must_compile.iter() {
            if compilations.contains_key(name) {
                match compilations[name] {
                    CompilationStatus::Done { .. } => {}
                    _ => panic!(
                        "Expecting {:?} to compile, but was {:?}",
                        name, compilations[name]
                    ),
                }
            } else {
                panic!("Expecting {:?} to compile, but was not in the UI", name);
            }
        }
        for name in self.must_not_compile.iter() {
            if compilations.contains_key(name) {
                match compilations[name] {
                    CompilationStatus::Failed { .. } => {}
                    _ => panic!(
                        "Expecting {:?} to not compile, but was {:?}",
                        name, compilations[name]
                    ),
                }
            } else {
                panic!("Expecting {:?} to not compile, but was not in the UI", name);
            }
        }
        for name in self.not_compiled.iter() {
            if compilations.contains_key(name) {
                panic!(
                    "Expecting {:?} not to be compiled, but was {:?}",
                    name, compilations[name]
                );
            }
        }
    }

    /// Check that the score of the subtasks are good.
    fn check_subtasks(&self, state: &UIState) {
        if let Some(scores) = &self.subtask_scores {
            assert_eq!(
                scores.len(),
                state.task.subtasks.len(),
                "Subtask len mismatch"
            );
            for i in 0..scores.len() {
                let expected = scores[i];
                let actual = state.task.subtasks[&(i as SubtaskId)].max_score;
                assert!(abs_diff_eq!(expected, actual), "Subtask score mismatch");
            }
        }
    }

    /// Check that the scores of the solutions are good.
    fn check_solution_scores(&self, state: &UIState) {
        let evaluations: HashMap<PathBuf, &SolutionEvaluationState> = state
            .evaluations
            .iter()
            .map(|(file, eval)| (PathBuf::from(file.file_name().unwrap()), eval))
            .collect();
        for (name, scores) in self.solution_scores.iter() {
            if !evaluations.contains_key(name) {
                panic!("No evaluation score for solution {:?}", name)
            }
            let state = evaluations[name];
            let score: f64 = scores.iter().sum();
            let state_score = state
                .score
                .unwrap_or_else(|| panic!("missing score of {:?}", name));
            assert!(
                abs_diff_eq!(score, state_score),
                "Solution score mismatch: {} != {}",
                score,
                state_score
            );
            assert_eq!(
                scores.len(),
                state.subtasks.len(),
                "Wrong number of subtask"
            );
            for st in 0..scores.len() {
                let expected = scores[st];
                let actual = state.subtasks[&(st as SubtaskId)].score.unwrap();
                assert!(
                    abs_diff_eq!(expected, actual),
                    "Solution subtask score mismatch: {} != {}",
                    expected,
                    actual
                );
            }
        }
    }

    /// Check that the statuses of the solutions are good.
    fn check_solution_statuses(&self, state: &UIState) {
        let evaluations: HashMap<PathBuf, Vec<TestcaseEvaluationStatus>> = state
            .evaluations
            .iter()
            .map(|(file, eval)| {
                (
                    PathBuf::from(file.file_name().unwrap()),
                    eval.subtasks
                        .keys()
                        .sorted()
                        .flat_map(|st| {
                            eval.subtasks[st]
                                .testcases
                                .keys()
                                .sorted()
                                .map(move |tc| eval.subtasks[st].testcases[tc].status.clone())
                        })
                        .collect(),
                )
            })
            .collect();
        for (name, statuses) in self.solution_statuses.iter() {
            if !evaluations.contains_key(name) {
                panic!("No evaluation statues for solution {:?}", name)
            }
            let actuals = &evaluations[name];
            for i in 0..actuals.len() {
                let actual = &actuals[i];
                let expected = if i < statuses.len() {
                    &statuses[i]
                } else {
                    &statuses[statuses.len() - 1]
                };
                assert_eq!(expected, actual, "Solution status mismatch of {:?}", name);
            }
        }
    }

    fn check_generations(&self, state: &UIState) {
        let generations: Vec<_> = state
            .generations
            .keys()
            .sorted()
            .flat_map(|st| {
                state.generations[st]
                    .testcases
                    .keys()
                    .sorted()
                    .map(move |tc| state.generations[st].testcases[tc].clone())
            })
            .collect();
        if let Some(statuses) = &self.generation_statuses {
            assert_eq!(
                statuses.len(),
                generations.len(),
                "Invalid number of testcases"
            );
            for (expected, testcase) in statuses.iter().zip(generations.iter()) {
                assert_eq!(expected, &testcase.status, "Testcase generation mismatch");
            }
        }
        if let Some(fails) = &self.generation_fails {
            assert_eq!(
                fails.len(),
                generations.len(),
                "Invalid number of testcases"
            );
            for (expected, testcase) in fails.iter().zip(generations.iter()) {
                if let Some(expected) = expected {
                    let gen_result = testcase.generation.as_ref().unwrap().clone();
                    let gen_stderr = testcase.generation_stderr.as_ref().unwrap().clone();
                    assert_ne!(
                        ExecutionStatus::Success,
                        gen_result.status,
                        "Expecting generation to fail"
                    );
                    assert!(
                        gen_stderr.contains(expected),
                        "Generation stderr does not contain {:?}",
                        expected
                    );
                }
            }
        }
        if let Some(fails) = &self.validation_fails {
            assert_eq!(
                fails.len(),
                generations.len(),
                "Invalid number of testcases"
            );
            for (expected, testcase) in fails.iter().zip(generations.iter()) {
                if let Some(expected) = expected {
                    let val_result = testcase.validation.as_ref().unwrap().clone();
                    let val_stderr = testcase.validation_stderr.as_ref().unwrap().clone();
                    assert_ne!(
                        ExecutionStatus::Success,
                        val_result.status,
                        "Expecting validation to fail"
                    );
                    assert!(
                        val_stderr.contains(expected),
                        "Validation stderr does not contain {:?}",
                        expected
                    );
                }
            }
        }
    }
}
