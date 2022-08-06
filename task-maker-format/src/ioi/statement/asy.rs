use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Error};
use regex::Regex;

use task_maker_dag::{Execution, ExecutionCommand, File};
use task_maker_diagnostics::{CodeSpan, Diagnostic};

use crate::UISender;
use crate::{bind_exec_callbacks, ui::UIMessage, EvaluationData, Tag};

lazy_static! {
    static ref ASY_INCLUDE: Regex =
        Regex::new(r#"(?:include|import)\s*['"]?([^'"\s]+)['"]?(?:\s+as\s+.+)?;"#)
            .expect("Invalid regex");
    static ref ASY_GRAPHIC: Regex =
        Regex::new(r#"(?:graphic|input)\s*\(\s*['"]([^'"]+)['"]"#).expect("Invalid regex");
}

pub struct AsyFile;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AsyDependency {
    pub sandbox_path: PathBuf,
    pub local_path: PathBuf,
    pub code_span: CodeSpan,
}

#[allow(clippy::derive_hash_xor_eq)]
impl Hash for AsyDependency {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.sandbox_path.hash(state);
    }
}

impl AsyFile {
    /// Compile the asy file and return the handle to the compiled and cropped pdf file.
    pub fn compile<P: Into<PathBuf>>(
        source: P,
        eval: &mut EvaluationData,
        booklet_name: &str,
    ) -> Result<File, Error> {
        let source_path = source.into();
        let booklet = booklet_name.to_string();
        let name = source_path
            .file_name()
            .ok_or_else(|| anyhow!("Invalid path of asy file: {:?}", source_path))?
            .to_string_lossy()
            .to_string();
        let source_file = File::new(format!("Source of {}", name));

        let mut comp = Execution::new(
            format!("Compilation of {}", name),
            ExecutionCommand::system("asy"),
        );
        comp.args(vec![
            "-f",
            "pdf",
            "-o",
            "output.pdf",
            "-localhistory", // This prevents "failed to create directory /.asy."
            "source.asy",
        ]);
        comp.limits_mut()
            .read_only(false)
            .wall_time(10.0) // asy tends to deadlock on failure
            .stack(8192 * 1024) // due to a libgc bug, asy may crash with unlimited stack
            .allow_multiprocess()
            .add_extra_readable_dir("/etc")
            .mount_tmpfs(true)
            .mount_proc(true);
        comp.tag(Tag::Booklet.into());
        comp.input(&source_file, "source.asy", false);
        eval.dag
            .provide_file(source_file, &source_path)
            .context("Failed to provide any source file")?;
        bind_exec_callbacks!(
            eval,
            comp.uuid,
            |status, booklet, name| UIMessage::IOIBookletDependency {
                booklet,
                name,
                step: 0,
                num_steps: 2,
                status
            },
            booklet,
            name
        )?;
        let deps = AsyFile::find_asy_deps(
            &source_path,
            source_path.parent().context("Invalid asy file")?,
        )
        .with_context(|| {
            format!(
                "Failed to find asy dependencies of {}",
                source_path.display()
            )
        })?;
        for dep in deps {
            if dep.local_path.exists() {
                let file = File::new(format!(
                    "Dependency {} of {}",
                    dep.sandbox_path.display(),
                    name
                ));
                comp.input(&file, &dep.sandbox_path, false);
                eval.dag
                    .provide_file(file, &dep.local_path)
                    .context("Failed to provide asy dependency")?;
            } else {
                let path = dep
                    .local_path
                    .strip_prefix(&eval.task_root)
                    .unwrap_or(&dep.local_path);
                eval.add_diagnostic(
                    Diagnostic::warning(format!(
                        "Failed to read {} used by {} because it was not found",
                        path.display(),
                        name
                    ))
                    .with_code_span(dep.code_span),
                )?;
            }
        }
        let compiled = comp.output("output.pdf");
        if eval.dag.data.config.copy_logs {
            let log_dir = eval.task_root.join("bin/logs/asy");
            let stderr_dest = log_dir.join(format!("{}.stderr.log", name));
            let stdout_dest = log_dir.join(format!("{}.stdout.log", name));
            eval.dag
                .write_file_to_allow_fail(comp.stderr(), stderr_dest, false);
            eval.dag
                .write_file_to_allow_fail(comp.stdout(), stdout_dest, false);
        }

        comp.capture_stderr(1024);
        eval.dag.on_execution_done(&comp.uuid, {
            let sender = eval.sender.clone();
            let name = name.clone();
            move |result| {
                if !result.status.is_success() {
                    let mut diagnostic = Diagnostic::error(format!("Failed to compile {}", name));
                    if result.status.is_internal_error() {
                        diagnostic = diagnostic.with_help("Is 'asymptote' installed?");
                    }
                    if let Some(stderr) = result.stderr {
                        diagnostic = diagnostic.with_help_attachment(stderr);
                    }
                    sender.add_diagnostic(diagnostic)?;
                }
                Ok(())
            }
        });
        eval.dag.add_execution(comp);

        let mut crop = Execution::new(
            format!("Crop of {}", name),
            ExecutionCommand::system("pdfcrop"),
        );
        crop.limits_mut()
            .read_only(false)
            .wall_time(10.0) // asy tends to deadlock on failure
            .allow_multiprocess()
            .add_extra_readable_dir("/etc")
            .mount_tmpfs(true);
        crop.tag(Tag::Booklet.into());
        crop.args(vec!["source.pdf"]);
        crop.input(compiled, "source.pdf", false);
        bind_exec_callbacks!(
            eval,
            crop.uuid,
            |status, booklet, name| UIMessage::IOIBookletDependency {
                booklet,
                name,
                step: 1,
                num_steps: 2,
                status
            },
            booklet,
            name
        )?;
        let cropped = crop.output("source-crop.pdf");
        crop.capture_stderr(1024);
        eval.dag.on_execution_done(&crop.uuid, {
            let sender = eval.sender.clone();
            move |result| {
                if !result.status.is_success() {
                    let mut diagnostic =
                        Diagnostic::error(format!("Failed to crop pdf of {}", name));
                    if result.status.is_internal_error() {
                        diagnostic = diagnostic.with_help("Is 'pdfcrop' installed?");
                    }
                    if let Some(stderr) = result.stderr {
                        diagnostic = diagnostic.with_help_attachment(stderr);
                    }
                    sender.add_diagnostic(diagnostic)?;
                }
                Ok(())
            }
        });
        eval.dag.add_execution(crop);

        Ok(cropped)
    }

    /// Recursively search for the asy dependencies of the specified file, where the sandbox
    /// directory is at the specified prefix.
    fn find_asy_deps(path: &Path, prefix: &Path) -> Result<HashSet<AsyDependency>, Error> {
        let dir = path
            .parent()
            .ok_or_else(|| anyhow!("File {:?} does not have a parent", path))?;
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read asy content from {}", path.display()))?;
        let mut result = HashSet::new();
        for include in ASY_INCLUDE.captures_iter(&content) {
            let match_ = include.get(1).unwrap();
            let code_span = CodeSpan::from_str(
                path,
                &content,
                match_.start(),
                match_.end() - match_.start(),
            )
            .context("Failed to build code span")?;
            let include = &include[1];
            // the filename might already have the ".asy" extension
            let extensions = ["", ".asy"];
            for ext in &extensions {
                let local_path = dir.join(include.to_owned() + ext);
                trace!("Checking probable asy dependency: {}", local_path.display());
                // may happen for example with `import math;`
                if !local_path.exists() {
                    continue;
                }
                let sandbox_path = local_path.strip_prefix(prefix)?;
                debug!(
                    "Asy dependency detected: {:?} -> {:?} = {:?}",
                    path, sandbox_path, local_path
                );
                result.extend(AsyFile::find_asy_deps(&local_path, prefix)?.into_iter());
                result.insert(AsyDependency {
                    sandbox_path: sandbox_path.into(),
                    local_path,
                    code_span,
                });
                break;
            }
        }
        for graphic in ASY_GRAPHIC.captures_iter(&content) {
            let match_ = graphic.get(1).unwrap();
            let code_span = CodeSpan::from_str(
                path,
                &content,
                match_.start(),
                match_.end() - match_.start(),
            )
            .context("Failed to build code span")?;
            let graphic = &graphic[1];
            let local_path = dir.join(graphic);
            let sandbox_path = local_path.strip_prefix(prefix)?;
            trace!(
                "Asy graphic detected: {:?} -> {:?} = {:?}",
                path,
                sandbox_path,
                local_path
            );
            result.insert(AsyDependency {
                sandbox_path: sandbox_path.into(),
                local_path,
                code_span,
            });
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use std::fs::write;

    use speculoos::prelude::*;

    use super::*;

    #[test]
    fn test_find_asy_deps() {
        let tmpdir = tempfile::TempDir::new().unwrap();
        let path = tmpdir.path().join("file.asy");
        let file_content = "import foo;";
        write(&path, file_content).unwrap();

        let foo_path = tmpdir.path().join("foo.asy");
        let foo_content = "import math;\ngraphic('img.png');";
        write(&foo_path, foo_content).unwrap();
        let deps = AsyFile::find_asy_deps(&path, tmpdir.path()).unwrap();

        assert_that(&deps.len()).is_equal_to(2);
        let dep1 = AsyDependency {
            sandbox_path: PathBuf::from("foo.asy"),
            local_path: foo_path.clone(),
            code_span: CodeSpan::from_str(&path, file_content, 7, 3).unwrap(),
        };
        assert!(deps.contains(&dep1), "{:#?} vs {:#?}", deps, dep1);
        let dep2 = AsyDependency {
            sandbox_path: PathBuf::from("img.png"),
            local_path: tmpdir.path().join("img.png"),
            code_span: CodeSpan::from_str(&foo_path, foo_content, 22, 7).unwrap(),
        };
        assert!(deps.contains(&dep2), "{:#?} vs {:#?}", deps, dep2);
    }

    #[test]
    fn test_asy_include_regex() {
        let tests = vec![
            (r#"import "file.asy" as foo;"#, "file.asy"),
            (r#"import 'file.asy' as foo;"#, "file.asy"),
            (r#"import file.asy as foo;"#, "file.asy"),
            (r#"import file as foo;"#, "file"),
            (r#"import "file";"#, "file"),
            (r#"import 'file';"#, "file"),
            (r#"import file;"#, "file"),
            ("import\tfile;", "file"),
            (r#"include "file.asy" as foo;"#, "file.asy"),
            (r#"include 'file.asy' as foo;"#, "file.asy"),
            (r#"include file.asy as foo;"#, "file.asy"),
            (r#"include file as foo;"#, "file"),
            (r#"include "file";"#, "file"),
            (r#"include 'file';"#, "file"),
            (r#"include file;"#, "file"),
            ("include\tfile;", "file"),
        ];
        for (line, path) in tests {
            let cap = ASY_INCLUDE.captures(line);
            if let Some(cap) = cap {
                if &cap[1] != path {
                    panic!("Expecting '{}' in '{}' but was '{}'", path, line, &cap[1]);
                }
            } else {
                panic!("Expecting '{}' in '{}' but nothing", path, line);
            }
        }
    }

    #[test]
    fn test_asy_graphics_regex() {
        let tests = vec![
            (r#"foo = graphic("file.png");"#, "file.png"),
            (r#"foo = graphic (   "file.png" );"#, "file.png"),
            (r#"foo = graphic('file.png');"#, "file.png"),
            (r#"foo = graphic (  'file.png' );"#, "file.png"),
            (r#"foo=graphic("file.png", 42);"#, "file.png"),
            (r#"foo=graphic('file.png', 42);"#, "file.png"),
            (r#"foo=input('file.txt');"#, "file.txt"),
            (r#"foo=input  (   'file.txt' );"#, "file.txt"),
            (r#"foo=input("file.txt");"#, "file.txt"),
            (r#"foo=input  (  "file.txt" );"#, "file.txt"),
            (r#"foo=input('file.txt', 42);"#, "file.txt"),
        ];
        for (line, path) in tests {
            let cap = ASY_GRAPHIC.captures(line);
            if let Some(cap) = cap {
                if &cap[1] != path {
                    panic!("Expecting '{}' in '{}' but was '{}'", path, line, &cap[1]);
                }
            } else {
                panic!("Expecting '{}' in '{}' but nothing", path, line);
            }
        }
    }
}
