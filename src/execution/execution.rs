use crate::execution::file::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub enum ExecutionCommand {
    System(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionInput {
    pub path: String,
    pub file: SharedFile,
    pub executable: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExecutionOutput {
    pub path: String,
    pub file: SharedFile,
}

pub struct ExecutionCallbacks {
    pub on_start: Option<Box<Fn() -> ()>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Execution {
    pub uuid: Uuid,
    pub description: String,
    pub command: ExecutionCommand,
    pub args: Vec<String>,

    pub stdin: Option<SharedFile>,
    pub stdout: Option<SharedFile>,
    pub inputs: Vec<ExecutionInput>,
    pub outputs: Vec<ExecutionOutput>,

    // separated because the functions are not derivable from Debug
    #[serde(skip)]
    pub callbacks: ExecutionCallbacks,
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
            inputs: vec![],
            outputs: vec![],

            callbacks: ExecutionCallbacks { on_start: None },
        }
    }

    pub fn stdin(&mut self, stdin: SharedFile) -> &mut Self {
        self.stdin = Some(stdin);
        self
    }

    pub fn stdout(&mut self) -> SharedFile {
        let file = File::new(&format!("Stdout of '{}'", self.description));
        self.stdout = Some(file.clone());
        file
    }

    pub fn input(&mut self, file: SharedFile, path: &str, executable: bool) -> &mut Self {
        self.inputs.push(ExecutionInput {
            path: path.to_owned(),
            file,
            executable,
        });
        self
    }

    pub fn output(&mut self, path: &str) -> SharedFile {
        let file = File::new(&format!("Output of '{}' at '{}'", self.description, path));
        self.outputs.push(ExecutionOutput {
            path: path.to_owned(),
            file: file.clone(),
        });
        file
    }

    pub fn on_start(&mut self, func: &'static Fn() -> ()) -> &mut Self {
        self.callbacks.on_start = Some(Box::new(func));
        self
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
        ExecutionCallbacks { on_start: None }
    }
}
