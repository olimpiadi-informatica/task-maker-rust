use std::path::{Path, PathBuf};

use anyhow::{Context, Error};

use task_maker_dag::{Execution, ExecutionCommand, ExecutionDAG, ExecutionLimits, File};

use crate::{Dependency, GraderMap};

/// Trait that defines the properties of the supported languages. Most of the methods have a safe
/// blanket implementation, note that not all of them are _really_ optional: based on the value
/// returned by `need_compilation` some of the methods become required.
///
/// A language can be either compiler or not-compiler.
///
/// When a language is compiled the extra required implementations are:
/// - `compilation_builder`
pub trait Language: std::fmt::Debug + Send + Sync {
    /// Full name of the language. This must be unique between all the other languages.
    fn name(&self) -> &'static str;

    /// List of valid extensions for this language. A file is considered in this language if its
    /// extension is inside this list.
    fn extensions(&self) -> Vec<&'static str>;

    /// Whether this language needs a compilation step. Returning `true` here triggers many changes
    /// in the behaviour of the execution. Of course the compilation step will be added, because of
    /// that there is the need to know how to compile the source file, forcing the implementation of
    /// some extra methods.
    fn need_compilation(&self) -> bool;

    /// The prefix to put at the start of a line to mark the whole line as a comment.
    ///
    /// The return value should include a space character only if required by the language.
    ///
    /// If the language does not support inline comments, return `None`.
    fn inline_comment_prefix(&self) -> Option<&'static str>;

    /// Return the `CompiledLanguageBuilder` for compiling a source file with this language.
    ///
    /// This method must return `Some` if and only if `need_compilation` returns `true`.
    fn compilation_builder(
        &self,
        _source: &Path,
        _settings: CompilationSettings,
    ) -> Option<Box<dyn CompiledLanguageBuilder + '_>> {
        None
    }

    /// Command to use to run the program. It defaults to the executable name of the program.
    /// Languages that need to run a separate program (e.g. a system-wise interpreter) may change
    /// the return value of this method.
    fn runtime_command(&self, path: &Path, write_to: Option<&Path>) -> ExecutionCommand {
        ExecutionCommand::local(self.executable_name(path, write_to))
    }

    /// Arguments to pass to the executable to start the evaluation.
    fn runtime_args(
        &self,
        _path: &Path,
        _write_to: Option<&Path>,
        args: Vec<String>,
    ) -> Vec<String> {
        args
    }

    /// Add a file to the runtime command if the language requires that. That file can be any run
    /// time dependency and it's relative to the sandbox.
    ///
    /// The new runtime arguments should be returned.
    fn runtime_add_file(&self, args: Vec<String>, _file: &Path) -> Vec<String> {
        args
    }

    /// The dependencies to put inside the execution sandbox. This does not include the executable.
    fn runtime_dependencies(&self, _path: &Path) -> Vec<Dependency> {
        vec![]
    }

    /// Update the limits for some language-specific requirements. For example the executable may
    /// need to fork (hence use more processes).
    fn custom_limits(&self, _limits: &mut ExecutionLimits) {}

    /// The name of the executable inside the sandbox. If this binary will be written elsewhere in
    /// the system, use the same name. Otherwise fallback to the original file name, without
    /// extension.
    fn executable_name(&self, path: &Path, write_to: Option<&Path>) -> PathBuf {
        if let Some(write_to) = write_to {
            PathBuf::from(write_to.file_name().expect("Invalid file name"))
        } else {
            PathBuf::from(path.file_stem().expect("Invalid file name"))
        }
    }
}

/// The generic settings that are common between all the compiled languages.
#[derive(Clone, Debug, Default)]
pub struct CompilationSettings {
    /// Where to write the compiled binary file.
    pub write_to: Option<PathBuf>,
    /// Whether to write to `write_to` the compiled binary.
    pub copy_exe: bool,
    /// Whether to try to link statically the binary.
    pub list_static: bool,
}

/// This trait describes the API of a "compiled language builder", a component that builds the DAG
/// execution for the compilation.
pub trait CompiledLanguageBuilder {
    /// If a grader map is present, provide it with this method.
    fn use_grader(&mut self, grader_map: &GraderMap);
    /// Build the execution to be added to the DAG for compiling the source file.
    ///
    /// This returns the execution to add and the file reference to the compiled binary file.
    ///
    /// After calling this method, the builder cannot be used anymore. This cannot be enforced by
    /// taking self because otherwise this trait is no longer object-safe.
    fn finalize(&mut self, dag: &mut ExecutionDAG) -> Result<(Execution, File), Error>;
}

/// A simple `CompiledLanguageBuilder` that is able to compile file in most of the languages.
///
/// It supports customizing the compiler, the command line arguments, a grader, a custom list of
/// additional dependencies, and also customizing the produced execution with a callback.
///
/// Some notes about the compilation:
///
/// - `language` must return true from `need_compilation`.
/// - `args` should not include the path to the source files (`finalize()` will add them)
/// - The compiler must write the binary file in `binary_name`, you can change this after
///   constructing the struct and before calling `finalize()`.
/// - The callback is called just before `finalize()` returns, so all the compilation properties
///   are already set.
/// - The `grader` will be set by `use_grader`, so your changes may be overwritten.
/// - After the first call to `finalize()` this struct cannot be used (see note on the definition
///   of `finalize` in the trait).
pub struct SimpleCompiledLanguageBuilder<'l, 'c> {
    /// A reference to the language that produced this builder.
    ///
    /// This is used to select which grader to use.
    pub language: &'l dyn Language,
    /// The settings for this compilation.
    pub settings: CompilationSettings,
    /// The local path to the source file to compile.
    pub source_path: PathBuf,
    /// The name of source file to compile.
    pub source_name: String,
    /// The name of the compiled file in the sandbox.
    pub binary_name: String,
    /// The compiler to use.
    pub compiler: ExecutionCommand,
    /// The list of arguments to pass to the compiler.
    ///
    /// To this list the paths to the source files are added automatically and should not be
    /// present.
    pub args: Vec<String>,
    /// The grader to use.
    pub grader: Option<Dependency>,
    /// The list of additional compilation dependencies.
    pub dependencies: Vec<Dependency>,
    /// Whether the compiler wants only the path to the grader file, or the paths to all the source
    /// files to compile together.
    pub grader_only: bool,
    /// The callback to call with the built `Execution` for additional customizations.
    pub callback: Option<Box<dyn FnOnce(&mut Execution) + 'c>>,
}

impl<'l, 'c> SimpleCompiledLanguageBuilder<'l, 'c> {
    /// Build a new `SimpleCompiledLanguageBuilder` for a source file in the given language.
    pub fn new<P: Into<PathBuf>>(
        language: &'l dyn Language,
        source_path: P,
        settings: CompilationSettings,
        compiler: ExecutionCommand,
    ) -> Self {
        let source_path = source_path.into();
        let mut source_name = source_path
            .file_name()
            .expect("Invalid file name")
            .to_string_lossy()
            .to_string();
        // names starting with - can be interpreted as command line options
        if source_name.starts_with('-') {
            source_name = format!("./{}", source_name);
        }
        Self {
            language,
            settings,
            source_path,
            source_name,
            binary_name: "__compiled".into(),
            compiler,
            args: Default::default(),
            grader: None,
            dependencies: Default::default(),
            grader_only: false,
            callback: None,
        }
    }

    /// Add a new argument to the list.
    pub fn add_arg<S: Into<String>>(&mut self, arg: S) -> &mut Self {
        self.args.push(arg.into());
        self
    }

    /// Add an additional dependency.
    pub fn add_dependency(&mut self, dependency: Dependency) {
        self.dependencies.push(dependency);
    }

    /// Pass only the path to the grader source file to the compiler, if the grader is present. If
    /// no grader is present, pass just the path to the actual source file.
    pub fn grader_only(&mut self) {
        self.grader_only = true;
    }

    /// Set the callback to call with the built execution.
    pub fn callback<F: FnOnce(&mut Execution) + 'c>(&mut self, callback: F) {
        self.callback = Some(Box::new(callback));
    }
}

impl<'l, 'c> CompiledLanguageBuilder for SimpleCompiledLanguageBuilder<'l, 'c> {
    fn use_grader(&mut self, grader_map: &GraderMap) {
        if let Some(grader) = grader_map.get_compilation_deps(self.language) {
            self.grader = Some(grader);
        }
    }

    fn finalize(&mut self, dag: &mut ExecutionDAG) -> Result<(Execution, File), Error> {
        let name = self.source_path.file_name().unwrap().to_string_lossy();
        let mut comp = Execution::new(format!("Compilation of {}", name), self.compiler.clone());
        comp.args = self.args.clone();

        // compilation dependencies
        for dep in self.dependencies.drain(..) {
            comp.input(&dep.file, &dep.sandbox_path, dep.executable);
            dag.provide_file(dep.file, &dep.local_path)
                .context("Failed to provide compilation dependency")?;
        }

        // main source file
        let source = File::new(format!("Source file of {:?}", self.source_path));
        comp.input(&source, &self.source_name, false);
        dag.provide_file(source, &self.source_path)
            .context("Failed to provide source file")?;

        // grader and sources in args
        if let Some(grader) = self.grader.take() {
            comp.args
                .push(grader.sandbox_path.to_string_lossy().to_string());
            comp.input(&grader.file, &grader.sandbox_path, grader.executable);
            dag.provide_file(grader.file, &grader.local_path)
                .context("Failed to provide grader dependency")?;

            if !self.grader_only {
                comp.args.push(self.source_name.clone());
            }
        } else {
            comp.args.push(self.source_name.clone());
        }

        // compiled binary
        let exec = comp.output(&self.binary_name);
        if self.settings.copy_exe {
            if let Some(write_to) = &self.settings.write_to {
                dag.write_file_to(&exec, write_to, true);
            }
        }

        if let Some(callback) = self.callback.take() {
            callback(&mut comp);
        }

        Ok((comp, exec))
    }
}
