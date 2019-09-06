use crate::ioi::Tag;
use crate::{bind_exec_callbacks, ui::UIMessage, EvaluationData};
use failure::{format_err, Error};
use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use task_maker_dag::{Execution, ExecutionCommand, File};

pub struct AsyFile;

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
            .ok_or_else(|| format_err!("Invalid path of asy file: {:?}", source_path))?
            .to_string_lossy()
            .to_string();
        let source_file = File::new(format!("Source of {}", name));

        let mut comp = Execution::new(
            format!("Compilation of {}", name),
            ExecutionCommand::system("asy"),
        );
        comp.args(vec!["-f", "pdf", "-o", "output.pdf", "source.asy"]);
        comp.limits_mut()
            .read_only(false)
            .wall_time(10.0) // asy tends to deadlock on failure
            .nproc(1000)
            .add_extra_readable_dir("/etc")
            .mount_tmpfs(true);
        comp.tag(Tag::Booklet.into());
        comp.input(&source_file, "source.asy", false);
        eval.dag.provide_file(source_file, &source_path)?;
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
        for (sandbox, local) in AsyFile::find_asy_deps(
            &source_path,
            &source_path.parent().expect("Invalid asy file"),
        )? {
            let file = File::new(format!("Dependency {:?} of {}", sandbox, name));
            comp.input(&file, sandbox, false);
            eval.dag.provide_file(file, local)?;
        }
        let compiled = comp.output("output.pdf");
        eval.dag.add_execution(comp);

        let mut crop = Execution::new(
            format!("Crop of {}", name),
            ExecutionCommand::system("pdfcrop"),
        );
        crop.limits_mut()
            .read_only(false)
            .wall_time(10.0) // asy tends to deadlock on failure
            .nproc(1000)
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
        eval.dag.add_execution(crop);

        Ok(cropped)
    }

    /// Recursively search for the asy dependencies of the specified file, where the sandbox
    /// directory is at the specified prefix.
    fn find_asy_deps(path: &Path, prefix: &Path) -> Result<HashMap<PathBuf, PathBuf>, Error> {
        lazy_static! {
            static ref ASY_INCLUDE: Regex =
                Regex::new(r"(?:include|import) *(.+)(?: +as +.+)?;").expect("Invalid regex");
            static ref ASY_GRAPHIC: Regex =
                Regex::new(r#"graphic\(['"]([^'"]+)['"]"#).expect("Invalid regex");
        }
        let dir = path
            .parent()
            .ok_or_else(|| format_err!("File {:?} does not have a parent", path))?;
        let content = std::fs::read_to_string(path)?;
        let mut result = HashMap::new();
        for include in ASY_INCLUDE.captures_iter(&content) {
            let include = &include[1];
            let local_path = dir.join(include.to_owned() + ".asy");
            // may happen for example with `import math;`
            if !local_path.exists() {
                continue;
            }
            let sandbox_path = local_path.strip_prefix(prefix)?;
            trace!(
                "Asy dependency detected: {:?} -> {:?} = {:?}",
                path,
                sandbox_path,
                local_path
            );
            result.extend(AsyFile::find_asy_deps(&local_path, prefix)?.into_iter());
            result.insert(sandbox_path.into(), local_path);
        }
        for graphic in ASY_GRAPHIC.captures_iter(&content) {
            let graphic = &graphic[1];
            let local_path = dir.join(graphic);
            let sandbox_path = local_path.strip_prefix(prefix)?;
            trace!(
                "Asy graphic detected: {:?} -> {:?} = {:?}",
                path,
                sandbox_path,
                local_path
            );
            result.insert(sandbox_path.into(), local_path);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spectral::prelude::*;
    use std::fs::write;

    #[test]
    fn test_find_asy_deps() {
        let tmpdir = tempdir::TempDir::new("tm-test").unwrap();
        let path = tmpdir.path().join("file.asy");
        let foo_path = tmpdir.path().join("foo.asy");
        write(&path, "import foo;").unwrap();
        write(&foo_path, "import math;\ngraphic('img.png');").unwrap();
        let deps = AsyFile::find_asy_deps(&path, tmpdir.path()).unwrap();
        assert_that!(deps).has_length(2);
        assert_that(&deps[Path::new("foo.asy")]).is_equal_to(&foo_path);
        assert_that(&deps[Path::new("img.png")]).is_equal_to(tmpdir.path().join("img.png"));
    }
}
