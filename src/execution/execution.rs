use crate::execution::file::*;
use crate::executor::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

pub type ExecutionUuid = Uuid;
pub type OnStartCallback = Fn(WorkerUuid) -> ();
pub type OnDoneCallback = Fn(WorkerResult) -> ();
pub type OnSkipCallback = Fn() -> ();

#[derive(Debug, Serialize, Deserialize)]
pub enum ExecutionCommand {
    System(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionInput {
    pub path: String,
    pub file: FileUuid,
    pub executable: bool,
}

pub struct ExecutionCallbacks {
    pub on_start: Option<Box<OnStartCallback>>,
    pub on_done: Option<Box<OnDoneCallback>>,
    pub on_skip: Option<Box<OnSkipCallback>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Execution {
    pub uuid: ExecutionUuid,
    pub description: String,
    pub command: ExecutionCommand,
    pub args: Vec<String>,

    pub stdin: Option<FileUuid>,
    pub stdout: Option<File>,
    pub stderr: Option<File>,
    pub inputs: Vec<ExecutionInput>,
    pub outputs: HashMap<String, File>,
}

impl Execution {
    pub fn new(description: &str, command: ExecutionCommand) -> Execution {
        Execution {
            uuid: Uuid::new_v4(),

            description: description.to_owned(),
            command,
            args: vec![],

            stdin: None,
            stdout: None,
            stderr: None,
            inputs: vec![],
            outputs: HashMap::new(),
        }
    }

    pub fn dependencies(&self) -> Vec<FileUuid> {
        let mut deps = vec![];
        if let Some(stdin) = self.stdin {
            deps.push(stdin);
        }
        for input in self.inputs.iter() {
            deps.push(input.file);
        }
        deps
    }

    pub fn outputs(&self) -> Vec<FileUuid> {
        let mut outs = vec![];
        if let Some(stdout) = &self.stdout {
            outs.push(stdout.uuid.clone());
        }
        if let Some(stderr) = &self.stderr {
            outs.push(stderr.uuid.clone());
        }
        for output in self.outputs.values() {
            outs.push(output.uuid.clone());
        }
        outs
    }

    pub fn stdin(&mut self, stdin: &File) -> &mut Self {
        self.stdin = Some(stdin.uuid.clone());
        self
    }

    pub fn stdout(&mut self) -> File {
        if self.stdout.is_none() {
            let file = File::new(&format!("Stdout of '{}'", self.description));
            self.stdout = Some(file);
        }
        self.stdout.as_ref().unwrap().clone()
    }

    pub fn stderr(&mut self) -> File {
        if self.stderr.is_none() {
            let file = File::new(&format!("Stderr of '{}'", self.description));
            self.stderr = Some(file);
        }
        self.stderr.as_ref().unwrap().clone()
    }

    pub fn input(&mut self, file: &File, path: &str, executable: bool) -> &mut Self {
        self.inputs.push(ExecutionInput {
            path: path.to_owned(),
            file: file.uuid.clone(),
            executable,
        });
        self
    }

    pub fn output(&mut self, path: &str) -> File {
        if self.outputs.contains_key(path) {
            return self.outputs.get(path).unwrap().clone();
        }
        let file = File::new(&format!("Output of '{}' at '{}'", self.description, path));
        self.outputs.insert(path.to_owned(), file);
        self.outputs.get(path).unwrap().clone()
    }
}

impl std::fmt::Debug for ExecutionCallbacks {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        formatter
            .debug_struct("ExecutionCallbacks")
            .field("on_start", &self.on_start.is_some())
            .field("on_done", &self.on_done.is_some())
            .field("on_skip", &self.on_skip.is_some())
            .finish()?;
        Ok(())
    }
}

impl std::default::Default for ExecutionCallbacks {
    fn default() -> Self {
        ExecutionCallbacks {
            on_start: None,
            on_done: None,
            on_skip: None,
        }
    }
}
