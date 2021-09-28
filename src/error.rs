use failure::{format_err, Error};
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
    let fail = error.as_fail();
    eprintln!("Error: {}", fail);
    for fail in fail.iter_causes() {
        eprintln!("Caused by: {}", fail);
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
                print_error(e.context(mex).into());
                std::process::exit(1);
            }
        }
    }

    fn nice_expect_with<S: Display + Send + Sync + 'static, F: FnOnce() -> S>(self, f: F) -> T {
        match self {
            Ok(x) => x,
            Err(e) => {
                print_error(e.context(f()).into());
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
                print_error(format_err!("Option is None"));
                std::process::exit(1);
            }
        }
    }

    fn nice_expect<S: Display + Send + Sync + 'static>(self, mex: S) -> T {
        match self {
            Some(x) => x,
            None => {
                print_error(format_err!("{}", mex));
                std::process::exit(1);
            }
        }
    }

    fn nice_expect_with<S: Display + Send + Sync + 'static, F: FnOnce() -> S>(self, f: F) -> T {
        match self {
            Some(x) => x,
            None => {
                print_error(format_err!("{}", f()));
                std::process::exit(1);
            }
        }
    }
}
