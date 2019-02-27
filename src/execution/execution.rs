use crate::execution::file::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type OnStartCallback = Fn(Uuid) -> ();
pub type OnDoneCallback = Fn(String) -> ();
pub type OnSkipCallback = Fn() -> ();

#[derive(Debug, Serialize, Deserialize)]
pub enum ExecutionCommand {
    System(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionInput {
    pub path: String,
    pub file: Uuid,
    pub executable: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionOutput {
    pub path: String,
    pub file: File,
}

pub struct ExecutionCallbacks {
    pub on_start: Option<Box<OnStartCallback>>,
    pub on_done: Option<Box<OnDoneCallback>>,
    pub on_skip: Option<Box<OnSkipCallback>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Execution {
    pub uuid: Uuid,
    pub description: String,
    pub command: ExecutionCommand,
    pub args: Vec<String>,

    pub stdin: Option<Uuid>,
    pub stdout: Option<File>,
    pub stderr: Option<File>,
    pub inputs: Vec<ExecutionInput>,
    pub outputs: Vec<ExecutionOutput>,
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
            outputs: vec![],
        }
    }

    pub fn stdin(&mut self, stdin: &File) -> &mut Self {
        self.stdin = Some(stdin.uuid.clone());
        self
    }

    pub fn stdout(&mut self) -> &File {
        if self.stdout.is_none() {
            let file = File::new(&format!("Stdout of '{}'", self.description));
            self.stdout = Some(file);
        }
        self.stdout.as_ref().unwrap()
    }

    pub fn stderr(&mut self) -> &File {
        if self.stderr.is_none() {
            let file = File::new(&format!("Stderr of '{}'", self.description));
            self.stderr = Some(file);
        }
        self.stderr.as_ref().unwrap()
    }

    pub fn input(&mut self, file: &File, path: &str, executable: bool) -> &mut Self {
        self.inputs.push(ExecutionInput {
            path: path.to_owned(),
            file: file.uuid.clone(),
            executable,
        });
        self
    }

    pub fn output(&mut self, path: &str) -> &File {
        let file = File::new(&format!("Output of '{}' at '{}'", self.description, path));
        self.outputs.push(ExecutionOutput {
            path: path.to_owned(),
            file: file,
        });
        &self.outputs.last().unwrap().file
    }
}

impl std::fmt::Debug for ExecutionCallbacks {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        formatter.write_fmt(format_args!("on_start: {}", self.on_start.is_some()))?;
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
