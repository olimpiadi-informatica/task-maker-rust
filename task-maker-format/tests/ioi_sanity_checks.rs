use std::process::Command;
use std::sync::Arc;
use task_maker_format::ioi::{Booklet, BookletConfig, IOITask, Statement, StatementConfig};
use task_maker_format::ui::UIMessage;
use task_maker_format::EvaluationData;
use task_maker_lang::GraderMap;

mod utils;

fn get_warnings(task: &IOITask) -> Vec<String> {
    let (mut eval, recv) = EvaluationData::new("");
    task.sanity_checks.pre_hook(task, &mut eval).unwrap();
    let mut res = vec![];
    while let Ok(mex) = recv.try_recv() {
        if let UIMessage::Diagnostic { diagnostic } = mex {
            res.push(diagnostic.to_string())
        }
    }
    res
}

fn get_post_warnings(task: &IOITask) -> Vec<String> {
    let (mut eval, recv) = EvaluationData::new("");
    task.sanity_checks.post_hook(task, &mut eval).unwrap();
    let mut res = vec![];
    while let Ok(mex) = recv.try_recv() {
        if let UIMessage::Diagnostic { diagnostic } = mex {
            res.push(diagnostic.to_string())
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

fn does_not_have_warning(warnings: &[String], warning: &str) {
    for warn in warnings {
        if warn.contains(warning) {
            panic!("{:?} contains {:?}", warnings, warning);
        }
    }
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
    let tmpdir = tempfile::TempDir::new().unwrap();
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
    let tmpdir = tempfile::TempDir::new().unwrap();
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
    let tmpdir = tempfile::TempDir::new().unwrap();
    let task = utils::new_task_with_context(tmpdir.path());
    std::fs::create_dir(tmpdir.path().join("att")).unwrap();

    let warnings = get_post_warnings(&task);
    has_warning(&warnings, "No sample file in att/");
}

#[test]
fn test_sanity_checks_att_sample_files_broken_link() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let task = utils::new_task_with_context(tmpdir.path());
    std::fs::create_dir(tmpdir.path().join("att")).unwrap();
    std::os::unix::fs::symlink("lololol", tmpdir.path().join("att/input0.txt")).unwrap();
    let warnings = get_post_warnings(&task);
    has_warning(&warnings, "Sample case att/input0.txt is a broken link");
}

#[test]
fn test_sanity_checks_att_sample_files_not_link() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let task = utils::new_task_with_context(tmpdir.path());
    std::fs::create_dir(tmpdir.path().join("att")).unwrap();
    std::fs::write(tmpdir.path().join("att/input0.txt"), "x").unwrap();
    let warnings = get_post_warnings(&task);
    has_warning(&warnings, "Sample case att/input0.txt is not a symlink");
}

#[test]
fn test_sanity_checks_duplicate_att_input() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let task = utils::new_task_with_context(tmpdir.path());
    std::fs::create_dir(tmpdir.path().join("att")).unwrap();
    std::fs::write(tmpdir.path().join("att/input0.txt"), "x").unwrap();
    std::fs::write(tmpdir.path().join("att/task.input0.txt"), "x").unwrap();
    let warnings = get_warnings(&task);
    has_warning(&warnings, "Sample input 0 is present more than once");
}

#[test]
fn test_sanity_checks_duplicate_att_output() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let task = utils::new_task_with_context(tmpdir.path());
    std::fs::create_dir(tmpdir.path().join("att")).unwrap();
    std::fs::write(tmpdir.path().join("att/output0.txt"), "x").unwrap();
    std::fs::write(tmpdir.path().join("att/task.output0.txt"), "x").unwrap();
    let warnings = get_warnings(&task);
    has_warning(&warnings, "Sample output 0 is present more than once");
}

#[test]
fn test_sanity_checks_att_input_without_output() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let task = utils::new_task_with_context(tmpdir.path());
    std::fs::create_dir(tmpdir.path().join("att")).unwrap();
    std::fs::write(tmpdir.path().join("att/input0.txt"), "x").unwrap();
    let warnings = get_warnings(&task);
    has_warning(
        &warnings,
        "Sample input file att/input0.txt does not have its output file",
    );
}

#[test]
fn test_sanity_checks_att_output_without_input() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let task = utils::new_task_with_context(tmpdir.path());
    std::fs::create_dir(tmpdir.path().join("att")).unwrap();
    std::fs::write(tmpdir.path().join("att/output0.txt"), "x").unwrap();
    let warnings = get_warnings(&task);
    has_warning(
        &warnings,
        "Sample output file att/output0.txt does not have its input file",
    );
}

#[test]
fn test_sanity_checks_att_with_check_rules() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let task = utils::new_task_with_context(tmpdir.path());
    std::fs::create_dir(tmpdir.path().join("att")).unwrap();
    std::fs::write(
        tmpdir.path().join("att/template.cpp"),
        "/*
 * @check-accepted: lel
 */
int main() {}",
    )
    .unwrap();
    let warnings = get_warnings(&task);
    has_warning(&warnings, "@check rule found in an attachment");
}

#[test]
fn test_sanity_checks_sol_graders() {
    let tmpdir = tempfile::TempDir::new().unwrap();
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
    let tmpdir = tempfile::TempDir::new().unwrap();
    let task = utils::new_task_with_context(tmpdir.path());
    std::fs::create_dir(tmpdir.path().join("sol")).unwrap();
    std::fs::write(tmpdir.path().join("sol/solution.cpp"), "x").unwrap();

    let warnings = get_warnings(&task);
    has_warning(&warnings, "Solution sol/solution.cpp is not a symlink");
}

#[test]
fn test_sanity_checks_statement_subtasks_oii_wrong() {
    let tmpdir = tempfile::TempDir::new().unwrap();
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
        "The score of subtask 2 in file.tex doesn't match the task's one",
    );
}

#[test]
fn test_sanity_checks_statement_subtasks_oii_out_of_order() {
    let tmpdir = tempfile::TempDir::new().unwrap();
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
        "The subtasks in file.tex are not sequentially numbered",
    );
}

#[test]
fn test_sanity_checks_statement_subtasks_ois_wrong() {
    let tmpdir = tempfile::TempDir::new().unwrap();
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
        "The score of subtask 1 in file.tex doesn't match the task's one",
    );
}

#[test]
fn test_sanity_checks_statement_valid_missing() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let task = utils::new_task_with_context(tmpdir.path());

    let warnings = get_post_warnings(&task);
    has_warning(&warnings, "Missing statement file");
}

#[test]
fn test_sanity_checks_statement_valid_invalid() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let task = utils::new_task_with_context(tmpdir.path());

    std::fs::create_dir(tmpdir.path().join("statement")).unwrap();
    std::fs::write(tmpdir.path().join("statement/statement.pdf"), "x").unwrap();

    let warnings = get_post_warnings(&task);
    has_warning(&warnings, "Invalid PDF file");
}

#[test]
fn test_sanity_checks_statement_valid_broken_link() {
    let tmpdir = tempfile::TempDir::new().unwrap();
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
    let tmpdir = tempfile::TempDir::new().unwrap();
    let task = utils::new_task_with_context(tmpdir.path());

    get_post_warnings(&task);
}

#[test]
fn test_sanity_checks_statement_git_everything_untracked() {
    let tmpdir = tempfile::TempDir::new().unwrap();
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
    does_not_have_warning(&warnings, "git");
}

#[test]
fn test_sanity_checks_statement_git_untracked() {
    let tmpdir = tempfile::TempDir::new().unwrap();
    let task = utils::new_task_with_context(tmpdir.path());

    std::fs::create_dir(tmpdir.path().join("statement")).unwrap();
    std::fs::write(tmpdir.path().join("statement/statement.pdf"), "%PDF").unwrap();
    std::fs::write(tmpdir.path().join("statement/english.tex"), "").unwrap();

    assert!(Command::new("git")
        .arg("init")
        .current_dir(tmpdir.path())
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .arg("add")
        .arg("statement/english.tex")
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
    let tmpdir = tempfile::TempDir::new().unwrap();
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
    assert!(Command::new("git")
        .arg("add")
        .arg(".gitignore")
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
    let tmpdir = tempfile::TempDir::new().unwrap();
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
        .arg("-f")
        .arg("statement/statement.pdf")
        .current_dir(tmpdir.path())
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap()
        .success());
    let warnings = get_post_warnings(&task);
    does_not_have_warning(&warnings, "git");
}
