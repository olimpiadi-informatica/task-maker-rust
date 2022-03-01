use std::path::Path;
use std::sync::Arc;

use task_maker_lang::GraderMap;

use crate::SourceFile;

/// A solution to evaluate. This includes the source file and some additional metadata.
#[derive(Clone, Debug)]
pub struct Solution {
    /// A reference to the source file of this solution.
    pub source_file: Arc<SourceFile>,
}

impl Solution {
    /// Create a new [`Solution`] for a given source file.
    ///
    /// Returns `None` if the language is unknown.
    pub fn new(path: &Path, base_dir: &Path, grader_map: Option<Arc<GraderMap>>) -> Option<Self> {
        let write_to = base_dir
            .join("bin")
            .join("sol")
            .join(path.file_name().unwrap());
        let source_file = SourceFile::new(path, base_dir, grader_map, Some(write_to))?;
        Some(Self {
            source_file: Arc::new(source_file),
        })
    }
}
