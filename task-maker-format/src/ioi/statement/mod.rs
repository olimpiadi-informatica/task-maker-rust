use anyhow::{anyhow, Context, Error};

pub use booklet::*;
pub use statement::*;

use crate::ioi::IOITask;
use crate::{list_files, EvaluationConfig};

mod asy;
mod booklet;
#[allow(clippy::module_inception)]
mod statement;

/// Find all the `Booklet` it makes sense to build.
pub fn make_booklets(
    task: &IOITask,
    eval_config: &EvaluationConfig,
) -> Result<Vec<Booklet>, Error> {
    let statements = list_files(&task.path, vec!["statement/*.tex", "testo/*.tex"]);
    let mut booklets = vec![];
    let config = StatementConfig::from_task(task);
    for path in statements {
        let dest = path.with_extension("pdf");
        let language = match path.file_stem() {
            Some(language) => language.to_string_lossy().to_string(),
            None => continue,
        };
        let statement = Statement::new(path, config.clone())
            .with_context(|| format!("Failed to build statement for language {}", language))?;
        let booklet_config = BookletConfig::from_contest(
            language,
            task.path
                .parent()
                .ok_or_else(|| anyhow!("Task is at the root"))?,
            eval_config.booklet_solutions,
        )
        .context("Failed to build booklet")?;
        let mut booklet = Booklet::new(booklet_config, dest);
        booklet.add_statement(statement);
        booklets.push(booklet);
    }
    Ok(booklets)
}
