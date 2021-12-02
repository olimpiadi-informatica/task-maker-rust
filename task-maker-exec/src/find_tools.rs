//! Finds the location of the `task-maker-tools` executable.

/// Locates the `task-maker-tools` executable.
pub fn find_tools_path() -> std::path::PathBuf {
    // Check environment variable.
    if let Some(path) = std::env::var_os("TASK_MAKER_TOOLS_PATH") {
        return path.into();
    }
    // Check in the directory of the current executable.
    let current_exe = std::env::current_exe();
    if let Ok(current_exe) = current_exe {
        let candidate_tools_path = current_exe.with_file_name("task-maker-tools");
        if candidate_tools_path.exists() {
            return candidate_tools_path;
        }
    }

    // Default to looking in PATH.
    "task-maker-tools".to_owned().into()
}
