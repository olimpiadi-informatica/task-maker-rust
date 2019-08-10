use crate::languages::*;

/// The Shell language
#[derive(Debug)]
pub struct LanguageShell;

impl LanguageShell {
    /// Make a new LanguageShell using the specified version.
    pub fn new() -> LanguageShell {
        LanguageShell {}
    }
}

impl Language for LanguageShell {
    fn name(&self) -> &'static str {
        "Shell"
    }

    fn extensions(&self) -> Vec<&'static str> {
        return vec!["sh"];
    }

    fn need_compilation(&self) -> bool {
        false
    }
}
