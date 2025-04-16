use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Error};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use task_maker_dag::{
    Execution, ExecutionDAG, ExecutionTag, ExecutionUuid, File, FileUuid, Priority,
};

use crate::language::{CompilationSettings, Language};
use crate::{GraderMap, LanguageManager};

/// Length of the stdout/stderr of the compilers to capture.
const COMPILATION_CONTENT_LENGTH: usize = 10 * 1024;
const COMPILATION_PRIORITY: Priority = 1_000_000_000;

/// A source file that will be able to be executed (with an optional compilation step).
///
/// After creating a `SourceFile` using `new` you can add start using it via the `execute` method.
/// Note that it may add to the DAG an extra execution for compiling the source file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFile {
    /// Path to the source file.
    pub path: PathBuf,
    /// Path to the base directory (e.g. the task root), used for including the source file
    /// dependencies from the command line args of the executable (in case of relative paths).
    pub base_path: PathBuf,
    /// Language of the source file.
    #[serde(serialize_with = "language_serializer")]
    #[serde(deserialize_with = "language_deserializer")]
    pub language: Arc<dyn Language>,
    /// Handle to the executable after the compilation/provided file.
    pub executable: Arc<Mutex<Option<File>>>,
    /// An optional handler to the map of the graders.
    pub grader_map: Option<Arc<GraderMap>>,
    /// Whether to force the copy-exe option of the DAG for this source file.
    pub copy_exe: bool,
    /// Where to write the compiled executable.
    pub write_bin_to: Option<PathBuf>,
    /// Whether this source file should be statically linked.
    pub link_static: bool,
}

impl SourceFile {
    /// Make a new `SourceFile` from the provided file. Will return `None` if the language is
    /// unknown.
    ///
    /// The language of the source file will be detected using the
    /// [`LanguageManager`](struct.LanguageManager.html), only those languages are supported.
    ///
    /// Because the execution/compilation may require some additional files a
    /// [`GraderMap`](struct.GraderMap.html) is required.
    pub fn new<P: Into<PathBuf>, P2: Into<PathBuf>, P3: Into<PathBuf>>(
        path: P,
        base_path: P2,
        grader_map: Option<Arc<GraderMap>>,
        write_bin_to: Option<P3>,
    ) -> Option<SourceFile> {
        let path = path.into();
        let base_path = base_path.into();
        let lang = LanguageManager::detect_language(&path);
        Some(SourceFile {
            path,
            base_path,
            language: lang?,
            executable: Arc::new(Mutex::new(None)),
            grader_map,
            write_bin_to: write_bin_to.map(|p| p.into()),
            copy_exe: false,
            link_static: false,
        })
    }

    /// Execute the program relative to this source file with the specified args. If the file has
    /// not been compiled yet this may add the compilation to the DAG. The compilation is added to
    /// the DAG only once for each `SourceFile` instance.
    ///
    /// The returned execution has the following properties already set:
    /// - the list of arguments
    /// - the input file for the executable
    /// - the input files from the language runtime dependencies
    /// - the input files from the grader map runtime dependencies
    ///
    /// The first element returned is the UUID of the execution of the compilation. It will be
    /// returned only once, even if the `execute` method is called more than once. The actual
    /// execution is returned as second parameter.
    ///
    /// The returned execution has all the dependencies already set, but it **has not been added**
    /// to the DAG yet. In order for this execution to work it has to be manually added to the DAG
    /// using [`ExecutionDAG::add_execution`](../task_maker_dag/struct.ExecutionDAG.html#method.add_execution).
    ///
    /// # Examples
    ///
    /// When executing a `.cpp` file the first item returned contains an handle to the compilation
    /// execution. Note that the second time the handle is not returned.
    ///
    /// ```
    /// use task_maker_dag::ExecutionDAG;
    /// use task_maker_lang::SourceFile;
    /// # use tempfile::TempDir;
    /// # use std::path::PathBuf;
    ///
    /// # let tempdir = TempDir::new().unwrap();
    /// # std::fs::write(tempdir.path().join("test.cpp"), "foobar.cpp").unwrap();
    /// # let path = tempdir.path().join("test.cpp");
    /// let mut dag = ExecutionDAG::new();
    /// let mut source = SourceFile::new(path /* test.cpp */, "", None, None::<PathBuf>).unwrap();
    ///
    /// let (comp, exec) = source.execute(&mut dag, "Execution", vec!["arg1".into()]).unwrap();
    /// assert!(comp.is_some());
    /// // customize the execution...
    /// dag.add_execution(exec);
    ///
    /// let (comp, exec) = source.execute(&mut dag, "Execution 2", vec!["arg1".into()]).unwrap();
    /// assert!(comp.is_none());
    /// dag.add_execution(exec);
    /// ```
    ///
    /// When executing a `.py` file the handle is not returned.
    ///
    /// ```
    /// use task_maker_dag::ExecutionDAG;
    /// use task_maker_lang::SourceFile;
    /// # use tempfile::TempDir;
    /// # use std::path::PathBuf;
    ///
    /// # let tempdir = TempDir::new().unwrap();
    /// # std::fs::write(tempdir.path().join("test.py"), "foobar.cpp").unwrap();
    /// # let path = tempdir.path().join("test.py");
    /// let mut dag = ExecutionDAG::new();
    /// let mut source = SourceFile::new(path /* test.py */, "", None, None::<PathBuf>).unwrap();
    ///
    /// let (comp, exec) = source.execute(&mut dag, "Execution", vec!["arg1".into()]).unwrap();
    /// assert!(comp.is_none());
    /// // customize the execution...
    /// dag.add_execution(exec);
    /// ```
    pub fn execute<S: AsRef<str>>(
        &self,
        dag: &mut ExecutionDAG,
        description: S,
        args: Vec<String>,
    ) -> Result<(Option<ExecutionUuid>, Execution), Error> {
        let comp = self.prepare(dag).context("Failed to prepare source file")?;
        let write_to = self.write_bin_to.as_deref();
        let mut exec = Execution::new(
            description.as_ref(),
            self.language.runtime_command(&self.path, write_to),
        );
        for arg in &args {
            let path = self.base_path.join(arg);
            if path.exists() {
                let file = File::new(format!(
                    "Command line dependency {:?} of {:?}",
                    path, self.path
                ));
                exec.input(&file, arg, false);
                dag.provide_file(file, path)
                    .context("Failed to provide input file from arg")?;
            }
        }
        exec.args(self.language.runtime_args(&self.path, write_to, args));
        exec.input(
            self.executable.lock().unwrap().as_ref().unwrap(),
            self.language.executable_name(&self.path, write_to),
            true,
        );
        for dep in self.language.runtime_dependencies(&self.path) {
            exec.input(&dep.file, &dep.sandbox_path, dep.executable);
            dag.provide_file(dep.file, &dep.local_path)
                .context("Failed to provide dependency")?;
        }
        if let Some(grader_map) = self.grader_map.as_ref() {
            for dep in grader_map.get_runtime_deps(self.language.as_ref()) {
                exec.input(&dep.file, &dep.sandbox_path, dep.executable);
                exec.args = self.language.runtime_add_file(exec.args, &dep.sandbox_path);
                dag.provide_file(dep.file, &dep.local_path)
                    .context("Failed to provide grader dependency")?;
            }
        }
        self.language.custom_limits(exec.limits_mut());
        // some languages (e.g. Python) may behave strangely with LC_ALL=C
        exec.copy_env("LANG");
        exec.copy_env("LC_ALL");
        Ok((comp, exec))
    }

    /// Force the executable to be copied to `write_bin_to` regardless of the option of the DAG.
    pub fn copy_exe(&mut self) {
        self.copy_exe = true;
    }

    /// Compile the source file to a statically-linked binary.
    pub fn link_static(&mut self) {
        // MacOS doesn't support statically linked binaries
        if cfg!(not(target_os = "macos")) {
            self.link_static = true;
        }
    }

    /// Prepare the source file if needed and return the executable file. If the compilation step
    /// was not executed yet the handle to the compilation execution is also returned.
    pub fn executable(
        &self,
        dag: &mut ExecutionDAG,
    ) -> Result<(FileUuid, Option<ExecutionUuid>), Error> {
        let comp = self.prepare(dag).context("Failed to prepare source file")?;
        let exe = self.executable.lock().unwrap().as_ref().unwrap().uuid;
        Ok((exe, comp))
    }

    /// The file name of the source file.
    ///
    /// ```
    /// use task_maker_lang::SourceFile;
    /// use std::path::PathBuf;
    ///
    /// let source = SourceFile::new("path/to/sourcefile.cpp", "", None, None::<PathBuf>).unwrap();
    ///
    /// assert_eq!(source.name(), "sourcefile.cpp");
    /// ```
    pub fn name(&self) -> String {
        self.path
            .file_name()
            .expect("Invalid file name")
            .to_string_lossy()
            .to_string()
    }

    /// The path to the file, relative to the `base_dir` if possible. If the file is not inside the
    /// base dir, the full path is returned.
    pub fn relative_path(&self) -> &Path {
        if let Ok(path) = self.path.strip_prefix(&self.base_path) {
            path
        } else {
            &self.path
        }
    }

    /// The optional destination of where to copy the executable if copy-exe option is set.
    ///
    /// ```
    /// use task_maker_lang::SourceFile;
    ///
    /// let source = SourceFile::new("path/to/sourcefile.cpp", "", None, Some("exec")).unwrap();
    ///
    /// assert_eq!(source.write_bin_to(), Some("exec".into()));
    /// ```
    pub fn write_bin_to(&self) -> Option<PathBuf> {
        self.write_bin_to.clone()
    }

    /// Prepare the source file setting the `executable` and eventually compiling the source file.
    pub fn prepare(&self, dag: &mut ExecutionDAG) -> Result<Option<ExecutionUuid>, Error> {
        if self.executable.lock().unwrap().is_some() {
            return Ok(None);
        }
        let write_to = self.write_bin_to.as_deref();
        let settings = CompilationSettings {
            write_to: write_to.map(Into::into),
            list_static: self.link_static,
            copy_exe: dag.config_mut().copy_exe || self.copy_exe,
        };
        if let Some(mut metadata) = self.language.compilation_builder(&self.path, settings) {
            if let Some(grader_map) = self.grader_map.as_ref() {
                metadata.use_grader(grader_map.as_ref());
            }
            let (mut comp, exec) = metadata.finalize(dag)?;
            comp.tag(ExecutionTag::from("compilation"))
                .priority(COMPILATION_PRIORITY)
                .capture_stdout(COMPILATION_CONTENT_LENGTH)
                .capture_stderr(COMPILATION_CONTENT_LENGTH);
            comp.limits
                .allow_multiprocess()
                // the compilers may need to store some temp files
                .read_only(false)
                .mount_tmpfs(true)
                .mount_proc(true);

            let comp_uuid = comp.uuid;
            dag.add_execution(comp);
            *self.executable.lock().unwrap() = Some(exec);
            Ok(Some(comp_uuid))
        } else {
            let executable = File::new(format!("Source file of {:?}", self.path));
            if dag.config_mut().copy_exe || self.copy_exe {
                if let Some(write_bin_to) = &self.write_bin_to {
                    dag.write_file_to(&executable, write_bin_to, true);
                }
            }
            *self.executable.lock().unwrap() = Some(executable.clone());
            dag.provide_file(executable, &self.path)
                .context("Failed to provide executable")?;
            Ok(None)
        }
    }

    /// The language of the source file.
    pub fn language(&self) -> &dyn Language {
        self.language.as_ref()
    }
}

/// Serializer for `Arc<dyn Language>`. It serializes just the name of the language, expecting the
/// deserializer to know how to deserialize it.
fn language_serializer<S>(lang: &Arc<dyn Language>, ser: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    ser.serialize_str(lang.name())
}

/// Deserializer for `Arc<dyn Language>`. It expects a `String` to be deserialized, searching in the
/// `LanguageManager` know languages the instance of that language.
fn language_deserializer<'de, D>(deser: D) -> Result<Arc<dyn Language>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;
    let lang_name = String::deserialize(deser)?;
    LanguageManager::from_name(lang_name).ok_or_else(|| D::Error::custom("unknown language"))
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use tempfile::TempDir;

    use task_maker_exec::{eval_dag_locally, SuccessSandboxRunner};

    use super::*;

    #[test]
    fn test_source_file_cpp() {
        let cwd = TempDir::new().unwrap();

        let mut dag = ExecutionDAG::new();
        dag.config_mut().copy_exe(true);

        let source = "int main() {return 0;}";
        let source_path = cwd.path().join("source.cpp");
        std::fs::File::create(&source_path)
            .unwrap()
            .write_all(source.as_bytes())
            .unwrap();
        let source = SourceFile::new(&source_path, "", None, Some(cwd.path().join("bin"))).unwrap();
        let (comp, exec) = source.execute(&mut dag, "Testing exec", vec![]).unwrap();
        assert!(comp.is_some());

        let exec_start = Arc::new(AtomicBool::new(false));
        let exec_start2 = exec_start.clone();
        let exec_done = Arc::new(AtomicBool::new(false));
        let exec_done2 = exec_done.clone();
        let exec_skipped = Arc::new(AtomicBool::new(false));
        let exec_skipped2 = exec_skipped.clone();
        dag.on_execution_start(&exec.uuid, move |_w| {
            exec_start2.store(true, Ordering::Relaxed);
            Ok(())
        });
        dag.on_execution_done(&exec.uuid, move |_res| {
            exec_done2.store(true, Ordering::Relaxed);
            Ok(())
        });
        dag.on_execution_skip(&exec.uuid, move || {
            exec_skipped2.store(true, Ordering::Relaxed);
            Ok(())
        });
        dag.add_execution(exec);

        eval_dag_locally(
            dag,
            cwd.path(),
            2,
            cwd.path(),
            1000,
            1000,
            SuccessSandboxRunner,
        );

        assert!(exec_start.load(Ordering::Relaxed));
        assert!(exec_done.load(Ordering::Relaxed));
        assert!(!exec_skipped.load(Ordering::Relaxed));
        assert!(cwd.path().join("bin").exists());
    }
}
