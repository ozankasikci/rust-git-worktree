use std::{ffi::OsStr, io, path::Path, process::Command};

use crate::telemetry::EditorLaunchStatus;

use super::EditorPreference;

pub struct LaunchRequest<'a> {
    pub preference: &'a EditorPreference,
    pub worktree_name: &'a str,
    pub worktree_path: &'a Path,
    pub wait_for_completion: bool,
}

#[derive(Debug, Clone)]
pub struct LaunchOutcome {
    pub status: EditorLaunchStatus,
    pub message: String,
}

pub fn launch_editor(request: LaunchRequest<'_>) -> LaunchOutcome {
    if !request.worktree_path.exists() {
        return LaunchOutcome {
            status: EditorLaunchStatus::InvalidWorktreePath,
            message: format!(
                "Worktree `{}` no longer exists at `{}`. Run `rsworktree worktree ls` or restart interactive mode to refresh the list.",
                request.worktree_name,
                request.worktree_path.display()
            ),
        };
    }

    let mut command = Command::new(&request.preference.command);
    command.args(&request.preference.args);
    command.arg(request.worktree_path);

    if request.wait_for_completion {
        // For interactive mode: wait for editor to complete
        match command.status() {
            Ok(status) => {
                if status.success() {
                    LaunchOutcome {
                        status: EditorLaunchStatus::Success,
                        message: format!(
                            "Launched `{}` using `{}`",
                            request.worktree_name,
                            format_command(&request.preference.command)
                        ),
                    }
                } else {
                    LaunchOutcome {
                        status: EditorLaunchStatus::SpawnError,
                        message: format!(
                            "Editor `{}` exited with status: {}",
                            format_command(&request.preference.command),
                            status
                        ),
                    }
                }
            }
            Err(error) => match error.kind() {
                io::ErrorKind::NotFound => LaunchOutcome {
                    status: EditorLaunchStatus::EditorMissing,
                    message: format!(
                        "Editor command `{}` was not found on PATH. Install the editor or update the configured command.",
                        format_command(&request.preference.command)
                    ),
                },
                _ => LaunchOutcome {
                    status: EditorLaunchStatus::SpawnError,
                    message: format!(
                        "Failed to launch `{}` via `{}`: {}",
                        request.worktree_name,
                        format_command(&request.preference.command),
                        error
                    ),
                },
            },
        }
    } else {
        // For non-interactive mode: spawn in background
        match command.spawn() {
            Ok(_) => LaunchOutcome {
                status: EditorLaunchStatus::Success,
                message: format!(
                    "Launched `{}` using `{}`",
                    request.worktree_name,
                    format_command(&request.preference.command)
                ),
            },
            Err(error) => match error.kind() {
                io::ErrorKind::NotFound => LaunchOutcome {
                    status: EditorLaunchStatus::EditorMissing,
                    message: format!(
                        "Editor command `{}` was not found on PATH. Install the editor or update the configured command.",
                        format_command(&request.preference.command)
                    ),
                },
                _ => LaunchOutcome {
                    status: EditorLaunchStatus::SpawnError,
                    message: format!(
                        "Failed to launch `{}` via `{}`: {}",
                        request.worktree_name,
                        format_command(&request.preference.command),
                        error
                    ),
                },
            },
        }
    }
}

fn format_command(command: &OsStr) -> String {
    command.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use tempfile::TempDir;

    use crate::editor::EditorPreference;

    #[test]
    fn reports_missing_worktree_path() {
        let request = LaunchRequest {
            preference: &EditorPreference {
                command: OsString::from("vim"),
                args: Vec::new(),
                source: crate::editor::EditorPreferenceSource::Environment {
                    variable: crate::editor::EditorEnvVar::Editor,
                },
            },
            worktree_name: "feature",
            worktree_path: Path::new("/nonexistent/path"),
            wait_for_completion: false,
        };

        let outcome = launch_editor(request);
        assert_eq!(outcome.status, EditorLaunchStatus::InvalidWorktreePath);
    }

    #[test]
    fn reports_missing_command() {
        let dir = TempDir::new().expect("tempdir");
        let worktree_path = dir.path();
        let request = LaunchRequest {
            preference: &EditorPreference {
                command: OsString::from("unlikely-editor-command"),
                args: Vec::new(),
                source: crate::editor::EditorPreferenceSource::Environment {
                    variable: crate::editor::EditorEnvVar::Editor,
                },
            },
            worktree_name: "feature",
            worktree_path,
            wait_for_completion: false,
        };

        let outcome = launch_editor(request);
        assert_eq!(outcome.status, EditorLaunchStatus::EditorMissing);
    }
}
