use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

pub type SharedFile = Arc<Mutex<File>>;

pub struct FileCallbacks {
    pub get_content: Option<(usize, Box<Fn(Vec<u8>) -> ()>)>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct File {
    pub uuid: Uuid,
    pub description: String,
    pub executable: bool,

    pub write_to: Option<String>,
    // separated because the functions are not derivable from Debug
    #[serde(skip)]
    pub callbacks: FileCallbacks,
}

impl File {
    pub fn new(description: &str) -> SharedFile {
        Arc::new(Mutex::new(File {
            uuid: Uuid::new_v4(),
            description: description.to_owned(),
            executable: false,

            write_to: None,
            callbacks: FileCallbacks { get_content: None },
        }))
    }

    pub fn get_content(
        &mut self,
        limit: usize,
        get_content: &'static Fn(Vec<u8>) -> (),
    ) -> &mut Self {
        self.callbacks.get_content = Some((limit, Box::new(get_content)));
        self
    }
}

impl std::fmt::Debug for FileCallbacks {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        formatter.write_fmt(format_args!("get_content: {}", self.get_content.is_some()))?;
        Ok(())
    }
}

impl std::default::Default for FileCallbacks {
    fn default() -> Self {
        FileCallbacks { get_content: None }
    }
}
