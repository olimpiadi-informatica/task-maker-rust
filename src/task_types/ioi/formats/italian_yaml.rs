use crate::task_types::ioi::formats::gen_gen::parse_gen_gen;
use crate::task_types::ioi::*;
use crate::task_types::*;
use failure::Error;
use std::path::Path;

/// italian_yaml format is structured as follow:
/// * task.yaml - file with the task information
/// * gen/ - folder with the generator and validator
/// *     generator... (also generatore)
/// *     validator... (also valida)
/// *     GEN - subtask and testcase specifications
/// * sol/ - folder with solutions, graders and stubs
/// *     solution... the official solution (also soluzione)
/// *     other...
/// * check/ - folder with the checker (also cor/)
/// *     checker... (also correttore)
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
        if path.join("gen").join("GEN").exists() {
            let (subtasks, testcases) = parse_gen_gen(&path.join("gen").join("GEN"))?;
            let info = IOITaskInfo {
                yaml,
                subtasks,
                testcases,
                checker: (),
            };
            info!("{:#?}", info);
            return Ok(Box::new(IOIBatchTask { info: info }));
        } else {
            // TODO static inputs
            unimplemented!();
        }
        unimplemented!();
    }
}
