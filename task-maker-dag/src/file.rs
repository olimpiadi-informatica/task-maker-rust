use std::path::PathBuf;

use anyhow::Error;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The identifier of a file, it's globally unique and it identifies a file
/// only during a single evaluation.
pub type FileUuid = Uuid;

/// Type of the callback called when a file is returned to the client.
pub type GetContentCallback = Box<dyn FnOnce(Vec<u8>) -> Result<(), Error> + 'static>;
/// Type of the callback called with the chunks of a file when it's ready.
pub type GetContentChunkedCallback = Box<dyn FnMut(&[u8]) -> Result<(), Error> + 'static>;

/// Where to write the file to with some other information.
#[derive(Debug, Clone)]
pub struct WriteToCallback {
    /// Destination path of the file to write.
    pub dest: PathBuf,
    /// Whether the file should be marked as executable.
    pub executable: bool,
    /// Whether this file is valid even if the execution that generated it failed.
    pub allow_failure: bool,
}

/// The callbacks that will trigger when the file is ready.
#[derive(Default)]
pub struct FileCallbacks {
    /// Destination of the file if it has to be stored in the disk of the client.
    pub write_to: Option<WriteToCallback>,
    /// Callback to be called with the first bytes of the file.
    pub get_content: Option<(usize, GetContentCallback)>,
    /// Callbacks to be called with the chunks of a file ready.
    pub get_content_chunked: Vec<GetContentChunkedCallback>,
}

/// An handle to a file in the evaluation, this only tracks dependencies between executions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd)]
pub struct File {
    /// Uuid of the file.
    pub uuid: FileUuid,
    /// Description of the file.
    pub description: String,
}

impl File {
    /// Create a new file handle.
    ///
    /// ```
    /// use task_maker_dag::File;
    ///
    /// let file = File::new("The output of the compilation");
    /// let uuid = file.uuid; // this is unique and it's the id of the file
    /// assert_eq!(file.description, "The output of the compilation");
    /// ```
    pub fn new<S: Into<String>>(description: S) -> File {
        File {
            uuid: Uuid::new_v4(),
            description: description.into(),
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

impl From<File> for FileUuid {
    fn from(file: File) -> Self {
        file.uuid
    }
}

impl From<&File> for FileUuid {
    fn from(file: &File) -> Self {
        file.uuid
    }
}

impl AsRef<FileUuid> for File {
    fn as_ref(&self) -> &FileUuid {
        &self.uuid
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file() {
        let file1 = File::new("file1");
        let file2 = File::new("file1");
        let file3 = File::new("file2");
        assert_ne!(file1, file2); // the uuid are different!
        assert_ne!(file1, file3);
    }
}
