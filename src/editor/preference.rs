use std::{
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::Repo;

pub const CONFIG_FILE_NAME: &str = "preferences.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorPreferenceResolution {
    Found(EditorPreference),
    Missing(PreferenceMissingReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorPreference {
    pub command: OsString,
    pub args: Vec<OsString>,
    pub source: EditorPreferenceSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorPreferenceSource {
    ConfigFile(PathBuf),
    Environment { variable: EditorEnvVar },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorEnvVar {
    Editor,
    Visual,
}

impl EditorEnvVar {
    pub fn name(self) -> &'static str {
        match self {
            EditorEnvVar::Editor => "EDITOR",
            EditorEnvVar::Visual => "VISUAL",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreferenceMissingReason {
    NotConfigured,
    ConfigInvalid {
        path: PathBuf,
        error: String,
    },
    EnvInvalid {
        variable: EditorEnvVar,
        error: String,
    },
}

#[derive(Debug, Deserialize)]
struct FileFormat {
    #[serde(default)]
    editor: Option<FileEditorPreference>,
}

#[derive(Debug, Deserialize)]
struct FileEditorPreference {
    command: String,
    #[serde(default)]
    args: Vec<String>,
}

pub fn resolve_editor_preference(repo: &Repo) -> color_eyre::Result<EditorPreferenceResolution> {
    let config_path = repo.worktrees_dir().join(CONFIG_FILE_NAME);

    if config_path.exists() {
        match load_from_config(&config_path) {
            Ok(Some(preference)) => {
                return Ok(EditorPreferenceResolution::Found(preference));
            }
            Ok(None) => {
                // Continue to environment fallback.
            }
            Err(reason) => {
                return Ok(EditorPreferenceResolution::Missing(reason));
            }
        }
    }

    for variable in [EditorEnvVar::Editor, EditorEnvVar::Visual] {
        match load_from_env(variable) {
            Ok(Some(preference)) => {
                return Ok(EditorPreferenceResolution::Found(preference));
            }
            Ok(None) => {
                // Try next source.
            }
            Err(reason) => {
                return Ok(EditorPreferenceResolution::Missing(reason));
            }
        }
    }

    Ok(EditorPreferenceResolution::Missing(
        PreferenceMissingReason::NotConfigured,
    ))
}

fn load_from_config(path: &Path) -> Result<Option<EditorPreference>, PreferenceMissingReason> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            return Err(PreferenceMissingReason::ConfigInvalid {
                path: path.to_path_buf(),
                error: error.to_string(),
            });
        }
    };

    let parsed: FileFormat =
        serde_json::from_str(&text).map_err(|error| PreferenceMissingReason::ConfigInvalid {
            path: path.to_path_buf(),
            error: error.to_string(),
        })?;

    let Some(editor) = parsed.editor else {
        return Ok(None);
    };

    if editor.command.trim().is_empty() {
        return Err(PreferenceMissingReason::ConfigInvalid {
            path: path.to_path_buf(),
            error: "`editor.command` must not be empty".to_string(),
        });
    }

    let mut args = Vec::with_capacity(editor.args.len());
    for arg in editor.args {
        args.push(OsString::from(arg));
    }

    Ok(Some(EditorPreference {
        command: OsString::from(editor.command),
        args,
        source: EditorPreferenceSource::ConfigFile(path.to_path_buf()),
    }))
}

fn load_from_env(
    variable: EditorEnvVar,
) -> Result<Option<EditorPreference>, PreferenceMissingReason> {
    let Some(raw_value) = env::var_os(variable.name()) else {
        return Ok(None);
    };

    if raw_value.is_empty() {
        return Ok(None);
    }

    let command_line = raw_value.to_str().map(|s| s.to_string()).ok_or_else(|| {
        PreferenceMissingReason::EnvInvalid {
            variable,
            error: "value contains non-UTF-8 characters".to_string(),
        }
    })?;

    let parts =
        shell_words::split(&command_line).map_err(|error| PreferenceMissingReason::EnvInvalid {
            variable,
            error: error.to_string(),
        })?;

    if parts.is_empty() {
        return Ok(None);
    }

    let mut parts_iter = parts.into_iter();
    let command = parts_iter.next().unwrap();
    let args = parts_iter.map(OsString::from).collect::<Vec<_>>();

    Ok(Some(EditorPreference {
        command: OsString::from(command),
        args,
        source: EditorPreferenceSource::Environment { variable },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use tempfile::TempDir;

    fn init_repo(dir: &TempDir) -> Repo {
        git2::Repository::init(dir.path()).expect("failed to init git repo");
        Repo::discover_from(dir.path()).expect("failed to discover repo")
    }

    #[test]
    fn resolves_preference_from_config_file() {
        let dir = TempDir::new().expect("tempdir");
        let repo = init_repo(&dir);
        let worktrees_dir = repo.ensure_worktrees_dir().expect("worktrees dir");
        let config_path = worktrees_dir.join(CONFIG_FILE_NAME);

        let json = serde_json::json!({
            "editor": {
                "command": "webstorm",
                "args": ["--line", "10"]
            }
        });
        fs::write(&config_path, serde_json::to_vec(&json).unwrap()).expect("write config");

        match resolve_editor_preference(&repo).expect("resolution") {
            EditorPreferenceResolution::Found(pref) => {
                assert_eq!(pref.command, OsString::from("webstorm"));
                assert_eq!(
                    pref.args,
                    vec![OsString::from("--line"), OsString::from("10")]
                );
                match pref.source {
                    EditorPreferenceSource::ConfigFile(path) => assert_eq!(path, config_path),
                    _ => panic!("expected config source"),
                }
            }
            other => panic!("unexpected resolution: {other:?}"),
        }
    }

    #[test]
    fn preference_missing_when_no_config_or_env() {
        let dir = TempDir::new().expect("tempdir");
        let repo = init_repo(&dir);

        let resolution = resolve_editor_preference(&repo).expect("resolution");
        assert!(matches!(
            resolution,
            EditorPreferenceResolution::Missing(PreferenceMissingReason::NotConfigured)
        ));
    }
}
