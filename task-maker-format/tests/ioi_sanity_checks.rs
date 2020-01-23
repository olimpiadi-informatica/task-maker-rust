use std::process::Command;
use std::sync::Arc;
use task_maker_format::ioi::{
    sanity_checks, Booklet, BookletConfig, Statement, StatementConfig, Task,
};
use task_maker_format::ui::UIMessage;
use task_maker_format::EvaluationData;
use task_maker_lang::GraderMap;

mod utils;

fn get_warnings(task: &Task) -> Vec<String> {
    let (mut eval, recv) = EvaluationData::new("");
    sanity_checks::pre_hook(&task, &mut eval).unwrap();
    let mut res = vec![];
    while let Ok(mex) = recv.try_recv() {
        if let UIMessage::Warning { message } = mex {
            res.push(message);
        }
    }
    res
}

fn get_post_warnings(task: &Task) -> Vec<String> {
    let (eval, recv) = EvaluationData::new("");
    sanity_checks::post_hook(&task, &mut eval.sender.lock().unwrap()).unwrap();
    let mut res = vec![];
    while let Ok(mex) = recv.try_recv() {
        if let UIMessage::Warning { message } = mex {
            res.push(message);
        }
    }
    res
}

fn has_warning(warnings: &[String], warning: &str) {
    for warn in warnings {
        if warn.contains(warning) {
            return;
        }
    }
    panic!("{:?} does not contain {:?}", warnings, warning);
}

#[test]
fn test_sanity_checks_max_score() {
    let mut task = utils::new_task();
    task.subtasks.get_mut(&0).unwrap().max_score = 111.0;
    let warnings = get_warnings(&task);
    has_warning(&warnings, "The score of the task");
}

#[test]
fn test_sanity_checks_att_graders() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    let mut task = utils::new_task_with_context(tmpdir.path());
    std::fs::create_dir(tmpdir.path().join("sol")).unwrap();
    std::fs::create_dir(tmpdir.path().join("att")).unwrap();
    std::fs::write(tmpdir.path().join("sol/grader.cpp"), "x").unwrap();
    std::fs::write(tmpdir.path().join("att/template.cpp"), "x").unwrap();
    task.grader_map = Arc::new(GraderMap::new(vec![tmpdir.path().join("sol/grader.cpp")]));

    let warnings = get_warnings(&task);
    has_warning(&warnings, "Missing grader at att/grader.cpp");
}

#[test]
fn test_sanity_checks_att_templates() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    let mut task = utils::new_task_with_context(tmpdir.path());
    std::fs::create_dir(tmpdir.path().join("sol")).unwrap();
    std::fs::create_dir(tmpdir.path().join("att")).unwrap();
    std::fs::write(tmpdir.path().join("sol/grader.cpp"), "x").unwrap();
    task.grader_map = Arc::new(GraderMap::new(vec![tmpdir.path().join("sol/grader.cpp")]));

    let warnings = get_warnings(&task);
    has_warning(&warnings, "Missing template at att/task.cpp");
}

#[test]
fn test_sanity_checks_att_sample_files_nothing() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    let task = utils::new_task_with_context(tmpdir.path());
    std::fs::create_dir(tmpdir.path().join("att")).unwrap();

    let warnings = get_warnings(&task);
    has_warning(&warnings, "No sample file in att/");
}

#[test]
fn test_sanity_checks_att_sample_files_broken_link() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    let task = utils::new_task_with_context(tmpdir.path());
    std::fs::create_dir(tmpdir.path().join("att")).unwrap();
    std::os::unix::fs::symlink("lololol", tmpdir.path().join("att/input0.txt")).unwrap();
    let warnings = get_warnings(&task);
    has_warning(&warnings, "Sample case att/input0.txt is a broken link");
}

#[test]
fn test_sanity_checks_att_sample_files_not_link() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    let task = utils::new_task_with_context(tmpdir.path());
    std::fs::create_dir(tmpdir.path().join("att")).unwrap();
    std::fs::write(tmpdir.path().join("att/input0.txt"), "x").unwrap();
    let warnings = get_warnings(&task);
    has_warning(&warnings, "Sample case att/input0.txt is not a symlink");
}

#[test]
fn test_sanity_checks_sol_graders() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    let mut task = utils::new_task_with_context(tmpdir.path());
    std::fs::create_dir(tmpdir.path().join("sol")).unwrap();
    std::fs::write(tmpdir.path().join("sol/grader.cpp"), "x").unwrap();
    std::fs::write(tmpdir.path().join("sol/template.cpp"), "x").unwrap();
    std::fs::write(tmpdir.path().join("sol/template.c"), "x").unwrap();
    task.grader_map = Arc::new(GraderMap::new(vec![tmpdir.path().join("sol/grader.cpp")]));

    let warnings = get_warnings(&task);
    has_warning(&warnings, "Missing grader at sol/grader.c");
}

#[test]
fn test_sanity_checks_sol_symlink() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    let task = utils::new_task_with_context(tmpdir.path());
    std::fs::create_dir(tmpdir.path().join("sol")).unwrap();
    std::fs::write(tmpdir.path().join("sol/solution.cpp"), "x").unwrap();

    let warnings = get_warnings(&task);
    has_warning(&warnings, "Solution sol/solution.cpp is not a symlink");
}

#[test]
fn test_sanity_checks_sol_unique() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    let task = utils::new_task_with_context(tmpdir.path());
    std::fs::create_dir(tmpdir.path().join("sol")).unwrap();
    std::fs::write(tmpdir.path().join("sol/solution.cpp"), "x").unwrap();
    std::fs::write(tmpdir.path().join("sol/solution.c"), "x").unwrap();

    let warnings = get_warnings(&task);
    has_warning(&warnings, "More than an official solution found");
}

#[test]
fn test_sanity_checks_statement_subtasks_oii_wrong() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    let mut task = utils::new_task_with_context(tmpdir.path());

    let tex = r"\item \textbf{\makebox[2cm][l]{Subtask 1} [\phantom{1}10 punti]}: $M=1$.\
                \item \textbf{\makebox[2cm][l]{Subtask 2} [\phantom{1}10 punti]}: $M=1$.";
    std::fs::write(tmpdir.path().join("file.tex"), tex).unwrap();

    let config = BookletConfig::default();
    let mut booklet = Booklet::new(config, tmpdir.path().join("foo.pdf"));
    let config = StatementConfig::from_task(&task);
    let statement = Statement::new(tmpdir.path().join("file.tex"), config).unwrap();
    booklet.add_statement(statement);
    task.booklets.push(booklet);

    let warnings = get_warnings(&task);
    has_warning(
        &warnings,
        "The subtasks in the statement file.tex don't match the tasks's ones",
    );
}

#[test]
fn test_sanity_checks_statement_subtasks_oii_out_of_order() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    let mut task = utils::new_task_with_context(tmpdir.path());

    let tex = r"\item \textbf{\makebox[2cm][l]{Subtask 2} [90 punti]}: $M=1$.\
                \item \textbf{\makebox[2cm][l]{Subtask 1} [10 punti]}: $M=1$.";
    std::fs::write(tmpdir.path().join("file.tex"), tex).unwrap();

    let config = BookletConfig::default();
    let mut booklet = Booklet::new(config, tmpdir.path().join("foo.pdf"));
    let config = StatementConfig::from_task(&task);
    let statement = Statement::new(tmpdir.path().join("file.tex"), config).unwrap();
    booklet.add_statement(statement);
    task.booklets.push(booklet);

    let warnings = get_warnings(&task);
    has_warning(
        &warnings,
        "The subtasks in the statement file.tex are non-sequentially numbered",
    );
}

#[test]
fn test_sanity_checks_statement_subtasks_ois_wrong() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    let mut task = utils::new_task_with_context(tmpdir.path());

    let tex = r"\OISubtask{10}{1}{$N \le 10$.}\
                \OISubtask{10}{1}{$N \le 10$.}";
    std::fs::write(tmpdir.path().join("file.tex"), tex).unwrap();

    let config = BookletConfig::default();
    let mut booklet = Booklet::new(config, tmpdir.path().join("foo.pdf"));
    let config = StatementConfig::from_task(&task);
    let statement = Statement::new(tmpdir.path().join("file.tex"), config).unwrap();
    booklet.add_statement(statement);
    task.booklets.push(booklet);

    let warnings = get_warnings(&task);
    has_warning(
        &warnings,
        "The subtasks in the statement file.tex don't match the tasks's ones",
    );
}

#[test]
fn test_sanity_checks_statement_valid_missing() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    let task = utils::new_task_with_context(tmpdir.path());

    let warnings = get_post_warnings(&task);
    has_warning(&warnings, "Missing statement file");
}

#[test]
fn test_sanity_checks_statement_valid_invalid() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    let task = utils::new_task_with_context(tmpdir.path());

    std::fs::create_dir(tmpdir.path().join("statement")).unwrap();
    std::fs::write(tmpdir.path().join("statement/statement.pdf"), "x").unwrap();

    let warnings = get_post_warnings(&task);
    has_warning(&warnings, "Invalid PDF file");
}

#[test]
fn test_sanity_checks_statement_valid_broken_link() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    let task = utils::new_task_with_context(tmpdir.path());

    std::fs::create_dir(tmpdir.path().join("statement")).unwrap();
    std::os::unix::fs::symlink("fooo", tmpdir.path().join("statement/statement.pdf")).unwrap();

    let warnings = get_post_warnings(&task);
    has_warning(
        &warnings,
        "Statement statement/statement.pdf is a broken link",
    );
}

#[test]
fn test_sanity_checks_statement_git_not_repo() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    let task = utils::new_task_with_context(tmpdir.path());

    get_post_warnings(&task);
}

#[test]
fn test_sanity_checks_statement_git_untracked() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    let task = utils::new_task_with_context(tmpdir.path());

    std::fs::create_dir(tmpdir.path().join("statement")).unwrap();
    std::fs::write(tmpdir.path().join("statement/statement.pdf"), "%PDF").unwrap();

    assert!(Command::new("git")
        .arg("init")
        .current_dir(tmpdir.path())
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap()
        .success());
    let warnings = get_post_warnings(&task);
    has_warning(
        &warnings,
        "File statement/statement.pdf is not known to git",
    );
}

#[test]
fn test_sanity_checks_statement_git_ignored() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    let task = utils::new_task_with_context(tmpdir.path());

    std::fs::create_dir(tmpdir.path().join("statement")).unwrap();
    std::fs::write(tmpdir.path().join("statement/statement.pdf"), "%PDF").unwrap();
    std::fs::write(tmpdir.path().join(".gitignore"), "*.pdf").unwrap();

    assert!(Command::new("git")
        .arg("init")
        .current_dir(tmpdir.path())
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap()
        .success());
    let warnings = get_post_warnings(&task);
    has_warning(
        &warnings,
        "File statement/statement.pdf is not known to git",
    );
}

#[test]
fn test_sanity_checks_statement_git_known() {
    let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
    let task = utils::new_task_with_context(tmpdir.path());

    std::fs::create_dir(tmpdir.path().join("statement")).unwrap();
    std::fs::write(tmpdir.path().join("statement/statement.pdf"), "%PDF").unwrap();

    assert!(Command::new("git")
        .arg("init")
        .current_dir(tmpdir.path())
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .arg("add")
        .arg("statement/statement.pdf")
        .current_dir(tmpdir.path())
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap()
        .success());
    let warnings = get_post_warnings(&task);
    assert!(warnings.is_empty());
}
