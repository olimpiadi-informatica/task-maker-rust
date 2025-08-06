use anyhow::{anyhow, Error};
use std::fmt::Display;

/// Adds methods for failing without panic. Like `expect` but without panic.
pub trait NiceError<T, E> {
    /// Fail exiting with `1` if the value is not present. Otherwise return the content.
    fn nice_unwrap(self) -> T;

    /// Fail exiting with `1` if the value is not present, printing to stderr the message. Otherwise
    /// return the content.
    fn nice_expect<S: Display + Send + Sync + 'static>(self, mex: S) -> T;

    /// Fail exiting with `1` if the value is not present, printing the message returned by the
    /// provided function. Otherwise return the content.
    fn nice_expect_with<S: Display + Send + Sync + 'static, F: FnOnce() -> S>(self, f: F) -> T;
}

fn print_error(error: Error) {
    debug!("{:?}", error);
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

    fn nice_expect<S: Display + Send + Sync + 'static>(self, mex: S) -> T {
        match self {
            Ok(x) => x,
            Err(e) => {
                print_error(e.context(mex));
                std::process::exit(1);
            }
        }
    }

    fn nice_expect_with<S: Display + Send + Sync + 'static, F: FnOnce() -> S>(self, f: F) -> T {
        match self {
            Ok(x) => x,
            Err(e) => {
                print_error(e.context(f()));
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

    fn nice_expect<S: Display + Send + Sync + 'static>(self, mex: S) -> T {
        match self {
            Some(x) => x,
            None => {
                print_error(anyhow!("{}", mex));
                std::process::exit(1);
            }
        }
    }

    fn nice_expect_with<S: Display + Send + Sync + 'static, F: FnOnce() -> S>(self, f: F) -> T {
        match self {
            Some(x) => x,
            None => {
                print_error(anyhow!("{}", f()));
                std::process::exit(1);
            }
        }
    }
}
