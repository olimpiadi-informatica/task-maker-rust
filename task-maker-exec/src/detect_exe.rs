use std::fs::File;
use std::io::Read;
use std::path::Path;

use failure::Error;

/// The platform of an executable.
#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub enum ExecutablePlatform {
    Linux,
    Windows,
    MacOs,
    MacOsFat,
}

/// The number of bits of the platform.
#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub enum ExecutableBits {
    Unknown,
    Bits32,
    Bits64,
}

/// A list of patterns for matching the header of the executables of the various platforms.
const PATTERNS: [(&[u8], (ExecutablePlatform, ExecutableBits)); 8] = [
    (
        b"\x4D\x5A",
        (ExecutablePlatform::Windows, ExecutableBits::Unknown),
    ),
    (b"#!", (ExecutablePlatform::Linux, ExecutableBits::Unknown)),
    (
        b"\xCE\xFA\xED\xFE",
        (ExecutablePlatform::MacOs, ExecutableBits::Bits32),
    ),
    (
        b"\xCF\xFA\xED\xFE",
        (ExecutablePlatform::MacOs, ExecutableBits::Bits64),
    ),
    (
        b"\xBE\xBA\xFE\xCA",
        (ExecutablePlatform::MacOsFat, ExecutableBits::Bits32),
    ),
    (
        b"\xBF\xBA\xFE\xCA",
        (ExecutablePlatform::MacOsFat, ExecutableBits::Bits64),
    ),
    (
        b"\x7F\x45\x4C\x46\x01",
        (ExecutablePlatform::Linux, ExecutableBits::Bits32),
    ),
    (
        b"\x7F\x45\x4C\x46\x02",
        (ExecutablePlatform::Linux, ExecutableBits::Bits64),
    ),
];

/// Given a path to a file, check if the file is a valid executable.
///
/// - If there is an error reading the file, `Err(_)` is returned.
/// - If the file is not recognized as an executable, `Ok(None)` is returned.
/// - Otherwise `Ok(Some((platform, bits)))` is returned.
pub fn detect_exe<P: AsRef<Path>>(
    path: P,
) -> Result<Option<(ExecutablePlatform, ExecutableBits)>, Error> {
    let mut file = File::open(path.as_ref())?;
    let mut header = vec![];
    for (bytes, res) in &PATTERNS {
        if header.len() < bytes.len() {
            let mut missing = vec![0u8; bytes.len() - header.len()];
            file.read_exact(&mut missing)?;
            header.append(&mut missing);
        }
        if header.starts_with(bytes) {
            return Ok(Some(*res));
        }
    }
    Ok(None)
}
