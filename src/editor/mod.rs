mod launch;
mod preference;
mod support;

use std::path::Path;

use crate::{Repo, telemetry::EditorLaunchStatus};

pub use launch::{LaunchOutcome, LaunchRequest, launch_editor};
pub use preference::{
    CONFIG_FILE_NAME, EditorEnvVar, EditorPreference, EditorPreferenceResolution,
    EditorPreferenceSource, PreferenceMissingReason, resolve_editor_preference,
};

pub use support::{SupportedEditor, supported_editor_commands};

pub fn launch_worktree(
    repo: &Repo,
    worktree_name: &str,
    worktree_path: &Path,
    wait_for_completion: bool,
) -> color_eyre::Result<LaunchOutcome> {
    let resolution = resolve_editor_preference(repo)?;
    let outcome = match resolution {
        EditorPreferenceResolution::Found(preference) => launch_editor(LaunchRequest {
            preference: &preference,
            worktree_name,
            worktree_path,
            wait_for_completion,
        }),
        EditorPreferenceResolution::Missing(reason) => missing_preference_outcome(reason),
    };

    Ok(outcome)
}

fn missing_preference_outcome(reason: PreferenceMissingReason) -> LaunchOutcome {
    match reason {
        PreferenceMissingReason::NotConfigured => {
            let supported = supported_editor_commands()
                .map(|(command, label)| format!("{label} (`{command}`)"))
                .collect::<Vec<_>>()
                .join(", ");
            LaunchOutcome {
                status: EditorLaunchStatus::PreferenceMissing,
                message: format!(
                    "No editor configured. Set one in `.rsworktree/{}` or export $EDITOR/$VISUAL. Supported commands: {}",
                    CONFIG_FILE_NAME, supported
                ),
            }
        }
        PreferenceMissingReason::ConfigInvalid { path, error } => LaunchOutcome {
            status: EditorLaunchStatus::ConfigurationError,
            message: format!(
                "Editor configuration `{}` is invalid: {}",
                path.display(),
                error
            ),
        },
        PreferenceMissingReason::EnvInvalid { variable, error } => LaunchOutcome {
            status: EditorLaunchStatus::ConfigurationError,
            message: format!(
                "Environment variable ${} could not be parsed: {}",
                variable.name(),
                error
            ),
        },
    }
}
