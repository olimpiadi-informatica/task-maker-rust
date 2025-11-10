use anyhow::{anyhow, Error};

/// Adds methods for failing without panic. Like `expect` but without panic.
pub trait NiceError<T, E> {
    /// Fail exiting with `1` if the value is not present. Otherwise return the content.
    fn nice_unwrap(self) -> T;
}

fn print_error(error: Error) {
    debug!("{error:?}");
    let mut fail: &dyn std::error::Error = error.as_ref();
    eprintln!("Error: {fail}");
    while let Some(cause) = fail.source() {
        eprintln!("\nCaused by:\n    {cause}");
        fail = cause;
    }
}

impl<T> NiceError<T, Error> for Result<T, Error> {
    fn nice_unwrap(self) -> T {
        match self {
            Ok(x) => x,
            Err(e) => {
                print_error(e);
                std::process::exit(1);
            }
        }
    }
}

impl<T> NiceError<T, ()> for Option<T> {
    fn nice_unwrap(self) -> T {
        match self {
            Some(x) => x,
            None => {
                print_error(anyhow!("Option is None"));
                std::process::exit(1);
            }
        }
    }
}
