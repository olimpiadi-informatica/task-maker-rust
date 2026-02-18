use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Error};
pub use booklet::*;
use itertools::Itertools;
use locale_codes::{country, language};
pub use statement::*;

use crate::ioi::IOITask;
use crate::{list_files, EvaluationConfig};

mod asy;
mod booklet;
#[allow(clippy::module_inception)]
mod statement;

/// List of languages supported by CMS for statements
pub const LANGUAGES: [&str; 61] = [
    "afrikaans",
    "arabic",
    "armenian",
    "azerbaijani",
    "belarusian",
    "bengali",
    "bosnian",
    "bulgarian",
    "catalan",
    "chinese",
    "croatian",
    "czech",
    "danish",
    "dutch",
    "english",
    "estonian",
    "filipino",
    "finnish",
    "french",
    "georgian",
    "german",
    "greek",
    "hebrew",
    "hindi",
    "hungarian",
    "icelandic",
    "indonesian",
    "irish",
    "italian",
    "japanese",
    "kazakh",
    "korean",
    "kyrgyz",
    "latvian",
    "lithuanian",
    "luxembourgish",
    "macedonian",
    "malay",
    "mongolian",
    "norwegian",
    "persian",
    "polish",
    "portuguese",
    "romanian",
    "russian",
    "serbian",
    "sinhala",
    "slovak",
    "slovene",
    "spanish",
    "swedish",
    "tajik",
    "tamil",
    "thai",
    "turkish",
    "turkmen",
    "ukrainian",
    "urdu",
    "uzbek",
    "vietnamese",
    "other",
];

/// Find all the `Booklet` it makes sense to build for a single task.
pub fn make_task_booklets(
    task: &IOITask,
    eval_config: &EvaluationConfig,
) -> Result<Vec<Booklet>, Error> {
    let statements = find_statement_files(&task.path);
    let mut booklets = vec![];
    let config = StatementConfig::from_task(task);

    let unique_languages = statements
        .iter()
        .map(|(lang, _path)| lang)
        .sorted()
        .unique()
        .count();

    if unique_languages != statements.len() {
        bail!("Booklet contains statements with two or more different extensions");
    }

    for (language, path) in statements {
        let dest = path.with_extension("pdf");
        let statement = Statement::new(path, config.clone())
            .with_context(|| format!("Failed to build statement for language {language}"))?;
        let booklet_config = BookletConfig::from_contest(
            language,
            task.path
                .parent()
                .ok_or_else(|| anyhow!("Task is at the root"))?,
            eval_config.booklet_solutions,
        )
        .context("Failed to build booklet")?;
        let mut booklet = Booklet::new(booklet_config, dest);
        booklet.add_statement(statement)?;
        booklets.push(booklet);
    }

    Ok(booklets)
}

/// Find all the `Booklet` it makes sense to build for all the provided tasks.
pub fn make_contest_booklets(
    tasks: &[IOITask],
    eval_config: &EvaluationConfig,
) -> Result<Vec<Booklet>, Error> {
    if tasks.is_empty() {
        return Ok(vec![]);
    }
    let contest_dir = tasks[0]
        .path
        .parent()
        .ok_or_else(|| anyhow!("Task is at the root"))?;
    // check all the tasks are in the same directory, so we are sure that they belong all to the
    // same contest.
    for task in tasks.iter() {
        let this_contest_dir = task
            .path
            .parent()
            .ok_or_else(|| anyhow!("Task is at the root"))?;
        if contest_dir != this_contest_dir {
            bail!("The tasks are not all in the same directory (i.e. different contests)");
        }
    }

    let statements = tasks
        .iter()
        .map(|task| (task, find_statement_files(task.path())))
        .collect_vec();
    let mut by_language: HashMap<_, Vec<_>> = HashMap::new();
    for (task, statements) in statements.into_iter() {
        for (language, path) in statements {
            by_language.entry(language).or_default().push((task, path));
        }
    }

    let mut booklets = vec![];
    for (language, tasks) in by_language {
        let booklet_config =
            BookletConfig::from_contest(&language, contest_dir, eval_config.booklet_solutions)
                .context("Failed to build booklet contest configuration")?;
        let dest = contest_dir.join(format!("{language}.pdf"));
        let mut booklet = Booklet::new(booklet_config, dest);

        for (task, path) in tasks {
            let config = StatementConfig::from_task(task);
            let statement = Statement::new(path, config)
                .with_context(|| format!("Failed to build statement for language {language}"))?;

            booklet.add_statement(statement)?;
        }
        booklets.push(booklet);
    }

    Ok(booklets)
}

/// Find a list of all the statement files for a task, extracting the language from them.
fn find_statement_files(task_dir: &Path) -> Vec<(String, PathBuf)> {
    list_files(
        task_dir,
        vec![
            "statement/*.tex",
            "statement/*.typ",
            "testo/*.tex",
            "testo/*.typ",
        ],
    )
    .into_iter()
    .filter_map(|path| {
        path.file_stem()
            .map(|lang| lang.to_string_lossy().to_string())
            .map(|lang| (lang, path))
    })
    .filter(|(lang, _path)| is_valid_statement_name(lang))
    .collect()
}

fn is_valid_statement_name(name: &str) -> bool {
    // if the file name is a language name recognized by cms, it is valid
    if LANGUAGES.contains(&name) {
        return true;
    }

    let sections = name.split(&['_', '.']).collect::<Vec<_>>();

    // If the filename has been split in more than two parts, it is not valid
    if sections.len() != 2 {
        return false;
    }

    let lang_ok =
        2 <= sections[0].len() && sections[0].len() <= 3 && language::lookup(sections[0]).is_some();
    let country_ok =
        2 <= sections[1].len() && sections[1].len() <= 3 && country::lookup(sections[1]).is_some()
            || sections[1] == "ISC";

    lang_ok && country_ok
}
