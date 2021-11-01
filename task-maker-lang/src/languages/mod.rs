use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};

use task_maker_dag::File;

use crate::Dependency;

pub(crate) mod c;
pub(crate) mod cpp;
pub(crate) mod pascal;
pub(crate) mod python;
pub(crate) mod shell;

/// Extract all the dependencies of a source file recursively. The file can include/import many
/// other files, even cyclically. Each import is included only once in the result.
///
/// ## Params
///
/// - `path`: the root source file from which extract the dependencies.
/// - `extract_imports`: a function called with the path of a source file, which returns a list of
///   all the imported files (pair local_path, sandbox_path).
pub(crate) fn find_dependencies<F>(path: &Path, mut extract_imports: F) -> Vec<Dependency>
where
    F: FnMut(&Path) -> Vec<(PathBuf, PathBuf)>,
{
    let base = path.parent().expect("Invalid path");
    let filename = path.file_name().expect("Invalid path");
    let mut result = vec![];
    let mut result_files = HashSet::new();
    let mut pending = VecDeque::new();
    let mut done = HashSet::new();

    pending.push_back(path.to_owned());
    while !pending.is_empty() {
        let path = pending.pop_front().unwrap();
        done.insert(path.clone());
        for (local_path, sandbox_path) in extract_imports(&path) {
            let local_path = base.join(&local_path);
            if local_path.exists()
                && !done.contains(&local_path)
                && !result_files.contains(&sandbox_path)
            {
                result_files.insert(sandbox_path.clone());
                pending.push_back(local_path.clone());
                result.push(Dependency {
                    file: File::new(&format!(
                        "Dependency {:?} at {:?} of {:?}",
                        sandbox_path, local_path, filename
                    )),
                    local_path,
                    sandbox_path,
                    executable: false,
                });
            }
        }
    }

    result
}
