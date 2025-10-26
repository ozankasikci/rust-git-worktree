use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorLaunchStatus {
    Success,
    EditorMissing,
    InvalidWorktreePath,
    SpawnError,
    PreferenceMissing,
    ConfigurationError,
}

pub fn log_editor_launch_attempt(
    worktree: &str,
    path: &Path,
    status: EditorLaunchStatus,
    message: &str,
) {
    eprintln!(
        "[open-editor] worktree={worktree} path={} status={status:?} message={message}",
        path.display()
    );
}
