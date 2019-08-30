mod asy;
mod booklet;
#[allow(clippy::module_inception)]
mod statement;

use crate::ioi::Task;
use crate::{list_files, EvaluationConfig};
pub use booklet::*;
use failure::Error;
pub use statement::*;
use std::path::PathBuf;

/// Directory where the data files are stored. It is taken from the `TM_DATA_DIR` environment
/// variable if present, otherwise it will be defaulted to the path of the source tree.
const DATA_DIR: Option<&str> = option_env!("TM_DATA_DIR");

/// Returns the path to the data directory where the files are stored.
fn data_dir_path() -> PathBuf {
    if let Some(dir) = DATA_DIR {
        dir.into()
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("data")
    }
}

/// Find all the `Booklet` it makes sense to build.
pub fn make_booklets(task: &Task, eval_config: &EvaluationConfig) -> Result<Vec<Booklet>, Error> {
    let statements = list_files(&task.path, vec!["statement/*.tex", "testo/*.tex"]);
    let mut booklets = vec![];
    let config = StatementConfig::from_task(task);
    for path in statements {
        let dest = path.with_extension("pdf");
        let language = match path.file_stem() {
            Some(language) => language.to_string_lossy().to_string(),
            None => continue,
        };
        let statement = Statement::new(path, config.clone())?;
        let booklet_config = BookletConfig::from_contest(
            language,
            task.path.parent().unwrap(),
            eval_config.booklet_solutions,
        )?;
        let mut booklet = Booklet::new(booklet_config, dest);
        booklet.add_statement(statement);
        booklets.push(booklet);
    }
    Ok(booklets)
}
