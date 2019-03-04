use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// The identifier of a file, it's globally unique and it identifies a file
/// only during a single evaluation.
pub type FileUuid = Uuid;

/// Type of the callback called when a file is returned to the client
pub type GetContentCallback = Fn(Vec<u8>) -> ();

/// Supported file callbacks
pub struct FileCallbacks {
    pub write_to: Option<PathBuf>,
    pub get_content: Option<(usize, Box<GetContentCallback>)>,
}

/// An handle to a file in the evaluation, this only tracks dependencies
/// between executions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct File {
    /// Uuid of the file
    pub uuid: FileUuid,
    /// Description of the file
    pub description: String,
}

impl File {
    /// Create a new file handle
    pub fn new(description: &str) -> File {
        File {
            uuid: Uuid::new_v4(),
            description: description.to_owned(),
        }
    }
}

impl std::fmt::Debug for FileCallbacks {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        formatter
            .debug_struct("FileCallbacks")
            .field("get_content", &self.get_content.is_some())
            .field("write_to", &self.write_to)
            .finish()?;
        Ok(())
    }
}

impl std::default::Default for FileCallbacks {
    fn default() -> FileCallbacks {
        FileCallbacks {
            write_to: None,
            get_content: None,
        }
    }
}
