use failure::_core::fmt::Debug;

/// Adds methods for failing without panic. Like `expect` but without panic.
pub trait NiceError<T, E> {
    /// Fail exiting with `1` if the value is not present, printing to stderr the message. Otherwise
    /// return the content.
    fn nice_expect(self, mex: &str) -> T;

    /// Fail exiting with `1` if the value is not present, printing the message returned by the
    /// provided function. Otherwise return the content.
    fn nice_expect_with<S: Into<String>, F: FnOnce(E) -> S>(self, f: F) -> T;
}

impl<T, E: Debug> NiceError<T, E> for Result<T, E> {
    fn nice_expect(self, mex: &str) -> T {
        match self {
            Ok(x) => x,
            Err(e) => {
                debug!("{:?}", e);
                eprintln!("{}", mex);
                std::process::exit(1);
            }
        }
    }

    fn nice_expect_with<S: Into<String>, F: FnOnce(E) -> S>(self, f: F) -> T {
        match self {
            Ok(x) => x,
            Err(e) => {
                debug!("{:?}", e);
                eprintln!("{}", f(e).into());
                std::process::exit(1);
            }
        }
    }
}

impl<T> NiceError<T, ()> for Option<T> {
    fn nice_expect(self, mex: &str) -> T {
        match self {
            Some(x) => x,
            None => {
                eprintln!("{}", mex);
                std::process::exit(1);
            }
        }
    }

    fn nice_expect_with<S: Into<String>, F: FnOnce(()) -> S>(self, f: F) -> T {
        match self {
            Some(x) => x,
            None => {
                eprintln!("{}", f(()).into());
                std::process::exit(1);
            }
        }
    }
}
