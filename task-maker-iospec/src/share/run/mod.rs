mod alloc;
mod ctx;
mod io;
mod state;
mod traits;

pub use alloc::*;
pub use ctx::*;
pub use io::*;
pub use state::*;
pub use traits::*;

use anyhow::Error;

pub enum Stop {
    Done,
    Error(Error),
}

impl<T: Into<Error>> From<T> for Stop {
    fn from(error: T) -> Self {
        Stop::Error(error.into())
    }
}

impl Stop {
    pub fn as_result(self) -> Result<(), Error> {
        match self {
            Stop::Done => Ok(()),
            Stop::Error(error) => Err(error),
        }
    }
}
