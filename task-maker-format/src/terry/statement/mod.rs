use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use typescript_definitions::TypeScriptify;

/// A statement is a markdown template together with subtasks data
#[derive(Debug, Clone, Serialize, Deserialize, TypeScriptify)]
pub struct Statement {
    /// The path of the statement template
    pub path: PathBuf,
    /// The subtasks if they exist
    pub subtasks: Option<PathBuf>,
}
