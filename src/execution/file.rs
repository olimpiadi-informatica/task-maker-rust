use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type FileUuid = Uuid;
pub type GetContentCallback = Fn(Vec<u8>) -> ();

pub struct FileCallbacks {
    pub write_to: Option<String>,
    pub get_content: Option<(usize, Box<GetContentCallback>)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct File {
    pub uuid: FileUuid,
    pub description: String,
    pub executable: bool,
}

impl File {
    pub fn new(description: &str) -> File {
        File {
            uuid: Uuid::new_v4(),
            description: description.to_owned(),
            executable: false,
        }
    }
}

impl std::fmt::Debug for FileCallbacks {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        formatter.write_fmt(format_args!(
            "get_content: {}, write_to: {:?}",
            self.get_content.is_some(),
            self.write_to
        ))?;
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
