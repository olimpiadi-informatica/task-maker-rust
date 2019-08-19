use std::path::{Path, PathBuf};
use std::sync::Arc;

use failure::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use task_maker_dag::*;

use crate::languages::*;
use crate::{GraderMap, LanguageManager};

/// A source file that will be able to be executed (with an optional compilation step).
///
/// After creating a `SourceFile` using `new` you can add start using it via the `execute` method.
/// Note that it may add to the DAG an extra execution for compiling the source file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFile {
    /// Path to the source file.
    pub path: PathBuf,
    /// Language of the source file.
    #[serde(serialize_with = "language_serializer")]
    #[serde(deserialize_with = "language_deserializer")]
    language: Arc<dyn Language>,
    /// Handle to the executable after the compilation/provided file.
    executable: Option<File>,
    /// An optional handler to the map of the graders.
    grader_map: Option<Arc<GraderMap>>,
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
    pub fn new<P: Into<PathBuf>>(
        path: P,
        grader_map: Option<Arc<GraderMap>>,
    ) -> Option<SourceFile> {
        let path = path.into();
        let lang = LanguageManager::detect_language(&path);
        lang.as_ref()?;
        Some(SourceFile {
            path,
            language: lang.unwrap(),
            executable: None,
            grader_map,
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
    /// # use tempdir::TempDir;
    ///
    /// # let tempdir = TempDir::new("tm-tests").unwrap();
    /// # std::fs::write(tempdir.path().join("test.cpp"), "foobar.cpp").unwrap();
    /// # let path = tempdir.path().join("test.cpp");
    /// let mut dag = ExecutionDAG::new();
    /// let mut source = SourceFile::new(path /* test.cpp */, None).unwrap();
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
    /// # use tempdir::TempDir;
    ///
    /// # let tempdir = TempDir::new("tm-tests").unwrap();
    /// # std::fs::write(tempdir.path().join("test.py"), "foobar.cpp").unwrap();
    /// # let path = tempdir.path().join("test.py");
    /// let mut dag = ExecutionDAG::new();
    /// let mut source = SourceFile::new(path /* test.py */, None).unwrap();
    ///
    /// let (comp, exec) = source.execute(&mut dag, "Execution", vec!["arg1".into()]).unwrap();
    /// assert!(comp.is_none());
    /// // customize the execution...
    /// dag.add_execution(exec);
    /// ```
    pub fn execute(
        &mut self,
        dag: &mut ExecutionDAG,
        description: &str,
        args: Vec<String>,
    ) -> Result<(Option<ExecutionUuid>, Execution), Error> {
        let comp = self.prepare(dag)?;
        let mut exec = Execution::new(description, self.language.runtime_command(&self.path));
        exec.args = self.language.runtime_args(&self.path, args);
        exec.input(
            self.executable.as_ref().unwrap(),
            &self.language.executable_name(&self.path),
            true,
        );
        for dep in self.language.runtime_dependencies(&self.path) {
            exec.input(&dep.file, &dep.sandbox_path, dep.executable);
            dag.provide_file(dep.file, &dep.local_path)?;
        }
        if let Some(grader_map) = self.grader_map.as_ref() {
            for dep in grader_map.get_runtime_deps(self.language.as_ref()) {
                exec.input(&dep.file, &dep.sandbox_path, dep.executable);
                exec.args = self.language.runtime_add_file(exec.args, &dep.sandbox_path);
                dag.provide_file(dep.file, &dep.local_path)?;
            }
        }
        Ok((comp, exec))
    }

    /// The file name of the source file.
    ///
    /// ```
    /// use task_maker_lang::SourceFile;
    /// let source = SourceFile::new("path/to/sourcefile.cpp", None).unwrap();
    ///
    /// assert_eq!(source.name(), "sourcefile.cpp");
    /// ```
    pub fn name(&self) -> String {
        String::from(self.path.file_name().unwrap().to_str().unwrap())
    }

    /// Prepare the source file setting the `executable` and eventually compiling the source file.
    fn prepare(&mut self, dag: &mut ExecutionDAG) -> Result<Option<ExecutionUuid>, Error> {
        if self.executable.is_some() {
            return Ok(None);
        }
        if self.language.need_compilation() {
            let mut comp = Execution::new(
                &format!("Compilation of {:?}", self.name()),
                self.language.compilation_command(&self.path),
            );
            comp.args = self.language.compilation_args(&self.path);
            let source = File::new(&format!("Source file of {:?}", self.path));
            comp.input(&source, Path::new(self.path.file_name().unwrap()), false);
            comp.limits.nproc = None;
            for dep in self.language.compilation_dependencies(&self.path) {
                comp.input(&dep.file, &dep.sandbox_path, dep.executable);
                dag.provide_file(dep.file, &dep.local_path)?;
            }
            if let Some(grader_map) = self.grader_map.as_ref() {
                for dep in grader_map.get_compilation_deps(self.language.as_ref()) {
                    comp.input(&dep.file, &dep.sandbox_path, dep.executable);
                    comp.args = self
                        .language
                        .compilation_add_file(comp.args, &dep.sandbox_path);
                    dag.provide_file(dep.file, &dep.local_path)?;
                }
            }
            let exec = comp.output(&self.language.executable_name(&self.path));
            let comp_uuid = comp.uuid;
            dag.add_execution(comp);
            dag.provide_file(source, &self.path)?;
            self.executable = Some(exec);
            Ok(Some(comp_uuid))
        } else {
            let executable = File::new(&format!("Source file of {:?}", self.path));
            self.executable = Some(executable.clone());
            dag.provide_file(executable, &self.path)?;
            Ok(None)
        }
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
    Ok(LanguageManager::from_name(lang_name).ok_or(D::Error::custom("unknown language"))?)
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    use tempdir::TempDir;

    use task_maker_exec::eval_dag_locally;

    use super::*;

    #[test]
    fn test_source_file_cpp() {
        let cwd = TempDir::new("tm-test").unwrap();

        let mut dag = ExecutionDAG::new();
        let source = "int main() {return 0;}";
        let source_path = cwd.path().join("source.cpp");
        std::fs::File::create(&source_path)
            .unwrap()
            .write_all(source.as_bytes())
            .unwrap();
        let mut source = SourceFile::new(&source_path, None).unwrap();
        let (comp, exec) = source.execute(&mut dag, "Testing exec", vec![]).unwrap();
        assert!(comp.is_some());

        let exec_start = Arc::new(Mutex::new(false));
        let exec_start2 = exec_start.clone();
        let exec_done = Arc::new(Mutex::new(false));
        let exec_done2 = exec_done.clone();
        let exec_skipped = Arc::new(Mutex::new(false));
        let exec_skipped2 = exec_skipped.clone();
        dag.on_execution_start(&exec.uuid, move |_w| {
            *exec_start2.lock().unwrap() = true;
        });
        dag.on_execution_done(&exec.uuid, move |_res| {
            *exec_done2.lock().unwrap() = true;
        });
        dag.on_execution_skip(&exec.uuid, move || {
            *exec_skipped2.lock().unwrap() = true;
        });
        dag.add_execution(exec);

        eval_dag_locally(dag, cwd.path(), 2);

        assert!(!*exec_skipped.lock().unwrap());
        assert!(*exec_start.lock().unwrap());
        assert!(*exec_done.lock().unwrap());
    }
}
