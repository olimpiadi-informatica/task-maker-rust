use crate::task_types::ioi::formats::gen_gen::parse_gen_gen;
use crate::task_types::ioi::*;
use failure::Error;
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
        path: &Path,
    ) -> Result<
        Box<Task<Self::SubtaskId, Self::TestcaseId, Self::SubtaskInfo, Self::TestcaseInfo>>,
        Error,
    > {
        let yaml = serde_yaml::from_reader::<_, IOITaskYAML>(std::fs::File::open(
            &path.join("task.yaml"),
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

        let sols = list_files(path, vec!["sol/*"]);
        let mut solutions = HashMap::new();
        let graders = list_files(path, vec!["sol/grader.*", "sol/stub.*"]);
        let grader_map = Arc::new(GraderMap::new(graders.clone()));
        warn!("The graders are {:?}", grader_map);
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
        let official = find_source_file(
            path,
            vec![
                "sol/solution.*",
                "sol/soluzione.*",
                "sol/solution",
                "sol/soluzione",
            ],
            Some(grader_map.clone()),
        );
        let official_solution = official.map(|s| {
            Box::new(IOISolution::new(
                Arc::new(s),
                infile,
                outfile,
                ExecutionLimits::default(), // the official solution does not have limits
            )) as Box<Solution<IOISubtaskId, IOITestcaseId>>
        });

        let get_score_type = |subtasks: &HashMap<IOISubtaskId, IOISubtaskInfo>,
                              testcases: &HashMap<
            IOISubtaskId,
            HashMap<IOITestcaseId, IOITestcaseInfo>,
        >| {
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
        };

        if path.join("gen").join("GEN").exists() {
            let (subtasks, testcases) = parse_gen_gen(&path.join("gen").join("GEN"))?;
            let info = IOITaskInfo {
                path: path.to_owned(),
                score_type: get_score_type(&subtasks, &testcases),
                yaml,
                subtasks,
                testcases,
                checker: Box::new(WhiteDiffChecker::new()),
            };
            let task = IOIBatchTask {
                info,
                solutions,
                official_solution,
            };
            info!("Task: {:#?}", task);
            Ok(Box::new(task))
        } else {
            // TODO static inputs
            unimplemented!();
        }
    }
}
