use boxfnonce::BoxFnOnce;
use failure::Error;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// The identifier of a file, it's globally unique and it identifies a file
/// only during a single evaluation.
pub type FileUuid = Uuid;

/// Type of the callback called when a file is returned to the client.
pub type GetContentCallback = BoxFnOnce<'static, (Vec<u8>,), Result<(), Error>>;

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
pub struct FileCallbacks {
    /// Destination of the file if it has to be stored in the disk of the client.
    pub write_to: Option<WriteToCallback>,
    /// Callback to be called with the first bytes of the file.
    pub get_content: Option<(usize, GetContentCallback)>,
}

/// An handle to a file in the evaluation, this only tracks dependencies between executions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, PartialOrd)]
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

impl std::default::Default for FileCallbacks {
    fn default() -> FileCallbacks {
        FileCallbacks {
            write_to: None,
            get_content: None,
        }
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
