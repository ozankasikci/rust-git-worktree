use std::path::{Path, PathBuf};

use color_eyre::eyre::{self, Context};

use crate::{
    Repo,
    commands::list::{find_worktrees, format_worktree},
    editor::launch_worktree,
    telemetry::{EditorLaunchStatus, log_editor_launch_attempt},
};

pub struct OpenEditorCommand {
    name: Option<String>,
    path: Option<PathBuf>,
}

impl OpenEditorCommand {
    pub fn new(name: Option<String>, path: Option<PathBuf>) -> Self {
        Self { name, path }
    }

    pub fn execute(&self, repo: &Repo) -> color_eyre::Result<()> {
        let resolved = self.resolve_target(repo)?;
        let outcome = match launch_worktree(repo, &resolved.name, &resolved.path, false) {
            Ok(outcome) => {
                log_editor_launch_attempt(
                    &resolved.name,
                    &resolved.path,
                    outcome.status,
                    &outcome.message,
                );
                outcome
            }
            Err(error) => {
                log_editor_launch_attempt(
                    &resolved.name,
                    &resolved.path,
                    EditorLaunchStatus::ConfigurationError,
                    &error.to_string(),
                );
                return Err(error);
            }
        };

        match outcome.status {
            EditorLaunchStatus::Success => {
                println!(
                    "Opened `{}` at `{}`.",
                    resolved.name,
                    resolved.path.display()
                );
                println!("{}", outcome.message);
                Ok(())
            }
            EditorLaunchStatus::PreferenceMissing => {
                println!("{}", outcome.message);
                Ok(())
            }
            _ => {
                eprintln!("{}", outcome.message);
                Err(eyre::eyre!(outcome.message))
            }
        }
    }

    fn resolve_target(&self, repo: &Repo) -> color_eyre::Result<ResolvedWorktree> {
        if let Some(path) = &self.path {
            return resolve_by_path(path, repo);
        }

        let name = self
            .name
            .as_ref()
            .ok_or_else(|| eyre::eyre!("worktree name or --path must be provided"))?;
        resolve_by_name(name, repo)
    }
}

struct ResolvedWorktree {
    name: String,
    path: PathBuf,
}

fn resolve_by_name(name: &str, repo: &Repo) -> color_eyre::Result<ResolvedWorktree> {
    let worktrees_dir = repo.ensure_worktrees_dir()?;
    let entries = find_worktrees(&worktrees_dir)?;

    let mut matches = Vec::new();

    for rel in entries {
        let display = format_worktree(&rel);
        let file_name = rel
            .file_name()
            .map(|component| component.to_string_lossy().into_owned());

        let is_match = display == name
            || display.ends_with(&format!("/{name}"))
            || file_name.as_deref() == Some(name);

        if is_match {
            matches.push((display, rel));
        }
    }

    if matches.is_empty() {
        return Err(eyre::eyre!(
            "worktree `{}` not found. Run `rsworktree ls` to view available worktrees.",
            name
        ));
    }

    if matches.len() > 1 {
        let names = matches
            .iter()
            .map(|(display, _)| display.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(eyre::eyre!(
            "worktree identifier `{}` is ambiguous. Matches: {}",
            name,
            names
        ));
    }

    let (display, rel) = matches.into_iter().next().unwrap();
    let absolute = worktrees_dir.join(&rel);

    if !absolute.exists() {
        return Err(eyre::eyre!(
            "worktree `{}` is missing from `{}`",
            display,
            absolute.display()
        ));
    }

    let canonical = absolute
        .canonicalize()
        .wrap_err_with(|| eyre::eyre!("failed to resolve `{}`", absolute.display()))?;

    Ok(ResolvedWorktree {
        name: display,
        path: canonical,
    })
}

fn resolve_by_path(path: &Path, repo: &Repo) -> color_eyre::Result<ResolvedWorktree> {
    if !path.exists() {
        return Err(eyre::eyre!(
            "worktree path `{}` does not exist",
            path.display()
        ));
    }

    let canonical = path
        .canonicalize()
        .wrap_err_with(|| eyre::eyre!("failed to resolve `{}`", path.display()))?;

    let worktrees_dir = repo.ensure_worktrees_dir()?;
    let display = if let Ok(relative) = canonical.strip_prefix(&worktrees_dir) {
        format_worktree(relative)
    } else if let Some(name) = canonical.file_name().and_then(|n| n.to_str()) {
        name.to_string()
    } else {
        canonical.display().to_string()
    };

    Ok(ResolvedWorktree {
        name: display,
        path: canonical,
    })
}
