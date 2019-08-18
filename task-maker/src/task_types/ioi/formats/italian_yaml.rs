use crate::evaluation::SourceFile;
use crate::task_types::ioi::formats::gen_gen::parse_gen_gen;
use crate::task_types::ioi::formats::static_inputs::static_inputs;
use crate::task_types::ioi::*;
use failure::{bail, Error};
use std::path::Path;

/// italian_yaml format is structured as follow:
/// * task.yaml - file with the task information
/// * gen/ - folder with the generator and validator
///     * generator... (also generatore)
///     * validator... (also valida)
///     * GEN - subtask and testcase specifications
/// * sol/ - folder with solutions, graders and stubs
///     * solution... the official solution (also soluzione)
///     * other...
/// * check/ - folder with the checker (also cor/)
///     * checker... (also correttore)
/// * input/ - folder with the input files
/// * output/ - folder with the output files
/// * statement/ - folder with the statement (also testo)
///
/// A task must have a generator (and a GEN file) or the input files should be
/// put in input/. The official solution must be present or the output files
/// should be put in output/.
pub struct IOIItalianYaml;

/// The iterator item type when following the task input testcases.
pub enum TaskInputEntry {
    Subtask {
        id: IOISubtaskId,
        info: IOISubtaskInfo,
    },
    GenerateTestcase {
        subtask: IOISubtaskId,
        id: IOITestcaseId,
        cmd: Vec<String>,
    },
    CopyTestcase {
        subtask: IOISubtaskId,
        id: IOITestcaseId,
        path: PathBuf,
    },
}

impl TaskFormat for IOIItalianYaml {
    type SubtaskId = IOISubtaskId;
    type TestcaseId = IOITestcaseId;
    type SubtaskInfo = IOISubtaskInfo;
    type TestcaseInfo = IOITestcaseInfo;

    /// Checks that there is at least one of gen/GEN or the input files.
    fn is_valid(path: &Path) -> bool {
        if !path.join("task.yaml").exists() {
            return false;
        }
        if path.join("gen").join("GEN").exists() {
            return true;
        }
        if path.join("input").exists() {
            return true;
        }
        false
    }

    /// Parse the task folder making one of the following task types:
    /// * IOIBatchTask
    fn parse(
        task_dir: &Path,
    ) -> Result<
        Box<Task<Self::SubtaskId, Self::TestcaseId, Self::SubtaskInfo, Self::TestcaseInfo>>,
        Error,
    > {
        let yaml = serde_yaml::from_reader::<_, IOITaskYAML>(std::fs::File::open(
            &task_dir.join("task.yaml"),
        )?)?;
        info!("The yaml is {:#?}", yaml);

        let infile = if yaml.infile != "" {
            Some(PathBuf::from(&yaml.infile))
        } else {
            None
        };
        let outfile = if yaml.outfile != "" {
            Some(PathBuf::from(&yaml.outfile))
        } else {
            None
        };
        let mut limits = ExecutionLimits::default();
        limits.cpu_time = yaml.time_limit;
        limits.memory = yaml.memory_limit.map(|l| l * 1024); // yaml is MiB, limits in KiB

        let sols = list_files(task_dir, vec!["sol/*"]);
        let mut solutions = HashMap::new();
        let graders = list_files(task_dir, vec!["sol/grader.*", "sol/stub.*"]);
        let grader_map = Arc::new(GraderMap::new(graders.clone()));

        let generator = find_generator(task_dir);
        let validator = find_validator(task_dir);
        let official_solution = find_official_solution(
            task_dir,
            grader_map.clone(),
            infile.clone(),
            outfile.clone(),
        );

        for sol in sols.into_iter().filter(|s| !graders.contains(s)) {
            let source = SourceFile::new(&sol, Some(grader_map.clone()));
            if let Some(source) = source {
                solutions.insert(
                    sol,
                    Box::new(IOISolution::new(
                        Arc::new(source),
                        infile.clone(),
                        outfile.clone(),
                        limits.clone(),
                    )) as Box<Solution<IOISubtaskId, IOITestcaseId>>,
                );
            }
        }

        let inputs = if task_dir.join("gen").join("GEN").exists() {
            parse_gen_gen(&task_dir.join("gen").join("GEN"))?
        } else {
            static_inputs(task_dir)?
        };

        let mut subtasks = HashMap::new();
        let mut testcases: HashMap<IOISubtaskId, HashMap<IOITestcaseId, IOITestcaseInfo>> =
            HashMap::new();
        for input in inputs {
            match input {
                TaskInputEntry::Subtask { id, info } => {
                    subtasks.insert(id, info);
                }
                TaskInputEntry::CopyTestcase { subtask, id, path } => {
                    testcases.entry(subtask).or_default().insert(
                        id,
                        IOITestcaseInfo {
                            testcase: id,
                            generator: Arc::new(StaticFileProvider::new(
                                format!("Static input of testcase {}", id),
                                path,
                            )),
                            validator: get_validator(&validator, subtask),
                            solution: get_solution(&official_solution, task_dir, id)?,
                        },
                    );
                }
                TaskInputEntry::GenerateTestcase { subtask, id, cmd } => {
                    testcases.entry(subtask).or_default().insert(
                        id,
                        IOITestcaseInfo {
                            testcase: id,
                            generator: Arc::new(IOIGenerator::new(generator.clone().unwrap(), cmd)),
                            validator: get_validator(&validator, subtask),
                            solution: get_solution(&official_solution, task_dir, id)?,
                        },
                    );
                }
            }
        }
        let info = IOITaskInfo {
            path: task_dir.to_owned(),
            score_type: get_score_type(&yaml, &subtasks, &testcases),
            yaml,
            subtasks,
            testcases,
            checker: Box::new(WhiteDiffChecker::new()),
        };
        let task = IOIBatchTask { info, solutions };
        info!("Task: {:#?}", task);
        Ok(Box::new(task))
    }
}

/// Search for the generator in the task directory.
fn find_generator(path: &Path) -> Option<Arc<SourceFile>> {
    find_source_file(
        path,
        vec![
            "gen/generator.*",
            "gen/generatore.*",
            "gen/generator",
            "gen/generatore",
        ],
        None,
    )
    .map(Arc::new)
}

/// Search for the validator in the task directory.
fn find_validator(path: &Path) -> Option<Arc<SourceFile>> {
    find_source_file(
        path,
        vec![
            "gen/validator.*",
            "gen/valida.*",
            "gen/validator",
            "gen/valida",
        ],
        None,
    )
    .map(Arc::new)
}

/// Search for the official solution in the task directory.
fn find_official_solution(
    path: &Path,
    grader_map: Arc<GraderMap>,
    infile: Option<PathBuf>,
    outfile: Option<PathBuf>,
) -> Option<Arc<Solution<IOISubtaskId, IOITestcaseId>>> {
    find_source_file(
        path,
        vec![
            "sol/solution.*",
            "sol/soluzione.*",
            "sol/solution",
            "sol/soluzione",
        ],
        Some(grader_map),
    )
    .map(|s| {
        Arc::new(IOISolution::new(
            Arc::new(s),
            infile,
            outfile,
            ExecutionLimits::default(), // the official solution does not have limits
        )) as Arc<Solution<IOISubtaskId, IOITestcaseId>>
    })
}

/// Returns the validator object relative to the specified subtask.
fn get_validator(
    validator: &Option<Arc<SourceFile>>,
    subtask: IOISubtaskId,
) -> Option<Arc<Validator<IOISubtaskId, IOITestcaseId>>> {
    validator.as_ref().map(|v| {
        Arc::new(IOIValidator::new(
            v.clone(),
            vec!["input.txt".to_string(), subtask.to_string()],
        )) as Arc<Validator<IOISubtaskId, IOITestcaseId>>
    })
}

/// Use the official solution if provided, otherwise use a static file
/// provider with the output file.
fn get_solution(
    official_solution: &Option<Arc<Solution<IOISubtaskId, IOITestcaseId>>>,
    path: &Path,
    testcase: IOITestcaseId,
) -> Result<Arc<Solution<IOISubtaskId, IOITestcaseId>>, Error> {
    if let Some(official) = official_solution.as_ref() {
        Ok(official.clone())
    } else {
        let static_file = path.join("output").join(format!("output{}.txt", testcase));
        if !static_file.exists() {
            bail!("Static output file does not exists! {:?}", static_file);
        }
        Ok(Arc::new(StaticFileProvider::new(
            format!("Static output of testcase {}", testcase),
            static_file,
        )) as Arc<Solution<IOISubtaskId, IOITestcaseId>>)
    }
}

/// Make the score type for this task.
fn get_score_type(
    yaml: &IOITaskYAML,
    subtasks: &HashMap<IOISubtaskId, IOISubtaskInfo>,
    testcases: &HashMap<IOISubtaskId, HashMap<IOITestcaseId, IOITestcaseInfo>>,
) -> Box<ScoreType<IOISubtaskId, IOITestcaseId>> {
    let subtasks = subtasks
        .iter()
        .map(|(id, s)| (*id, s as &SubtaskInfo))
        .collect();
    let testcases = testcases
        .iter()
        .map(|(id, s)| {
            (
                *id,
                s.iter()
                    .map(|(id, t)| (*id, t as &TestcaseInfo<IOISubtaskId, IOITestcaseId>))
                    .collect(),
            )
        })
        .collect();
    match yaml.score_type.as_ref().map(|s| s.as_str()) {
        None | Some("min") => Box::new(ScoreTypeMin::new(subtasks, testcases)),
        _ => unimplemented!(),
    }
}
