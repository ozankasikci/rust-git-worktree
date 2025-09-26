use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
};

use color_eyre::eyre::{self, WrapErr};
use owo_colors::{OwoColorize, Stream};

use crate::Repo;

#[derive(Debug, Default)]
pub struct ListCommand;

impl ListCommand {
    pub fn execute(&self, repo: &Repo) -> color_eyre::Result<()> {
        let worktrees_dir = repo.ensure_worktrees_dir()?;
        let worktrees = find_worktrees(&worktrees_dir)?;

        let header_path_raw = format!("{}", worktrees_dir.display());
        let header_path = format!(
            "{}",
            header_path_raw
                .as_str()
                .if_supports_color(Stream::Stdout, |text| { format!("{}", text.blue().bold()) })
        );
        let header_raw = format!("Worktrees under `{}`:", header_path);
        let header = format!(
            "{}",
            header_raw
                .as_str()
                .if_supports_color(Stream::Stdout, |text| format!("{}", text.bold()))
        );
        println!("{}", header);

        if worktrees.is_empty() {
            let message = format!(
                "{}",
                "(none)".if_supports_color(Stream::Stdout, |text| { format!("{}", text.dimmed()) })
            );
            println!("{}", message);
        } else {
            for worktree in worktrees {
                let entry_raw = format_worktree(&worktree);
                let entry = format!(
                    "{}",
                    entry_raw
                        .as_str()
                        .if_supports_color(Stream::Stdout, |text| { format!("{}", text.green()) })
                );
                println!("- {}", entry);
            }
        }

        Ok(())
    }
}

pub(crate) fn find_worktrees(base: &Path) -> color_eyre::Result<Vec<PathBuf>> {
    let mut results = Vec::new();
    let mut queue = VecDeque::new();
    queue.push_back(base.to_path_buf());

    while let Some(dir) = queue.pop_front() {
        for entry in fs::read_dir(&dir)
            .wrap_err_with(|| eyre::eyre!("failed to read `{}`", dir.display()))?
        {
            let entry = entry.wrap_err("failed to read directory entry")?;
            let path = entry.path();
            if entry
                .file_type()
                .wrap_err("failed to read entry file type")?
                .is_dir()
            {
                if path.join(".git").exists() {
                    let rel = path.strip_prefix(base).wrap_err_with(|| {
                        eyre::eyre!(
                            "failed to compute worktree path relative to `{}`",
                            base.display()
                        )
                    })?;
                    results.push(rel.to_path_buf());
                } else {
                    queue.push_back(path);
                }
            }
        }
    }

    results.sort();
    Ok(results)
}

pub(crate) fn format_worktree(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, process::Command};

    use tempfile::TempDir;

    use crate::Repo;

    fn init_git_repo(dir: &TempDir) -> color_eyre::Result<()> {
        run(dir, ["git", "init"])
    }

    fn run(dir: &TempDir, cmd: impl IntoIterator<Item = &'static str>) -> color_eyre::Result<()> {
        let mut iter = cmd.into_iter();
        let program = iter.next().expect("command must not be empty");
        let status = Command::new(program)
            .current_dir(dir.path())
            .args(iter)
            .status()
            .wrap_err_with(|| eyre::eyre!("failed to run `{program}`"))?;

        if !status.success() {
            return Err(eyre::eyre!("`{program}` exited with status {status}"));
        }

        Ok(())
    }

    #[test]
    fn lists_worktrees_recursively_in_alpha_order() -> color_eyre::Result<()> {
        let repo_dir = TempDir::new()?;
        init_git_repo(&repo_dir)?;
        let repo = Repo::discover_from(repo_dir.path())?;
        let worktrees_dir = repo.ensure_worktrees_dir()?;

        let worktree_a = worktrees_dir.join("feature/test");
        fs::create_dir_all(&worktree_a)?;
        fs::write(worktree_a.join(".git"), "gitdir: ../..")?;

        let worktree_b = worktrees_dir.join("bugfix/squash");
        fs::create_dir_all(&worktree_b)?;
        fs::write(worktree_b.join(".git"), "gitdir: ../..")?;

        let found = find_worktrees(&worktrees_dir)?;
        let labels: Vec<String> = found.iter().map(|path| format_worktree(path)).collect();

        assert_eq!(labels, vec!["bugfix/squash", "feature/test"]);

        Ok(())
    }
}
