//! TODO:
//! Eventually place this module in task-maker-exec crate and resolve the signal name directly in
//! the sandbox. Resolving it here may lead to wrong results on different OS.

use std::ffi::CStr;

mod unix {
    use std::os::raw::c_char;

    extern "C" {
        /// http://man7.org/linux/man-pages/man3/strsignal.3.html
        pub fn strsignal(signal: i32) -> *mut c_char;
    }
}

/// Returns a string with the text representation of the signal.
pub(crate) fn strsignal(signal: u32) -> String {
    #[cfg(unix)]
    {
        unsafe {
            let cstr = CStr::from_ptr(unix::strsignal(signal as i32));
            cstr.to_string_lossy().to_string()
        }
    }
    #[cfg(not(unix))]
    {
        "unknown".into()
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn test_strsignal() {
        assert_eq!("Segmentation fault", &strsignal(11));
    }
}
