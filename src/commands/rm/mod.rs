use std::{fs, path::Path, process::Command};

use color_eyre::eyre::{self, Context};
use owo_colors::{OwoColorize, Stream};

use git2::{BranchType, ErrorCode, WorktreePruneOptions};

use crate::{Repo, commands::cd::shell_command};

#[cfg(test)]
use crate::commands::cd::SHELL_OVERRIDE_ENV;

#[derive(Debug)]
pub struct RemoveCommand {
    name: String,
    force: bool,
    quiet: bool,
    remove_local_branch: bool,
}

impl RemoveCommand {
    pub fn new(name: String, force: bool) -> Self {
        Self {
            name,
            force,
            quiet: false,
            remove_local_branch: false,
        }
    }

    pub fn with_quiet(mut self, quiet: bool) -> Self {
        self.quiet = quiet;
        self
    }

    pub fn with_remove_local_branch(mut self, remove: bool) -> Self {
        self.remove_local_branch = remove;
        self
    }

    pub fn execute(&self, repo: &Repo) -> color_eyre::Result<()> {
        let worktrees_dir = repo.worktrees_dir();
        if !worktrees_dir.exists() {
            let dir = format!("{}", worktrees_dir.display());
            let dir = format!(
                "{}",
                dir.as_str()
                    .if_supports_color(Stream::Stdout, |text| format!("{}", text.blue()))
            );
            if !self.quiet {
                println!(
                    "No worktrees directory found at `{}`; nothing to remove.",
                    dir
                );
            }
            return Ok(());
        }

        let worktree_path = worktrees_dir.join(&self.name);
        let worktree_path = fs::canonicalize(&worktree_path).unwrap_or(worktree_path);

        if !worktree_path.exists() {
            let name = format!(
                "{}",
                self.name
                    .as_str()
                    .if_supports_color(Stream::Stdout, |text| format!("{}", text.cyan()))
            );
            if !self.quiet {
                println!(
                    "Worktree `{}` does not exist under `{}`.",
                    name,
                    worktrees_dir.display()
                );
            }
            return Ok(());
        }

        let git_repo = repo.git();
        let worktree_name = match find_worktree_name(git_repo, &worktree_path)? {
            Some(name) => name,
            None => {
                let name = format!(
                    "{}",
                    self.name
                        .as_str()
                        .if_supports_color(Stream::Stdout, |text| format!("{}", text.cyan()))
                );
                if !self.quiet {
                    println!(
                        "Worktree `{}` does not exist under `{}`.",
                        name,
                        worktrees_dir.display()
                    );
                }
                return Ok(());
            }
        };

        let worktree = git_repo.find_worktree(&worktree_name).wrap_err_with(|| {
            eyre::eyre!("failed to load git worktree metadata for `{}`", self.name)
        })?;

        let mut prune_opts = WorktreePruneOptions::new();
        prune_opts.valid(true);
        prune_opts.working_tree(true);
        if self.force {
            prune_opts.locked(true);
        }

        worktree
            .prune(Some(&mut prune_opts))
            .wrap_err("failed to remove worktree")?;

        drop(worktree);

        if worktree_path.exists() {
            fs::remove_dir_all(&worktree_path).wrap_err_with(|| {
                eyre::eyre!(
                    "failed to clean worktree directory `{}`",
                    worktree_path.display()
                )
            })?;
        }

        let name = format!(
            "{}",
            self.name
                .as_str()
                .if_supports_color(Stream::Stdout, |text| format!("{}", text.red().bold()))
        );
        if !self.quiet {
            println!(
                "Removed worktree `{}` from `{}`.",
                name,
                worktrees_dir.display()
            );
        }

        if self.remove_local_branch {
            self.delete_local_branch(repo)?;
        }

        let need_reposition = match std::env::current_dir() {
            Ok(dir) => {
                let canonical = fs::canonicalize(&dir).unwrap_or(dir.clone());
                canonical.starts_with(&worktree_path)
            }
            Err(_) => true,
        };

        if need_reposition {
            std::env::set_current_dir(repo.root()).wrap_err_with(|| {
                eyre::eyre!(
                    "failed to change directory to repository root `{}`",
                    repo.root().display()
                )
            })?;

            let root_raw = format!("{}", repo.root().display());
            let root_display = format!(
                "{}",
                root_raw
                    .as_str()
                    .if_supports_color(Stream::Stdout, |text| format!("{}", text.blue().bold()))
            );
            if !self.quiet {
                println!("Now in root `{}`.", root_display);
            }

            let (program, args) = shell_command();
            let status = Command::new(&program)
                .args(args)
                .current_dir(repo.root())
                .env("PWD", logical_pwd(repo.root()))
                .status()
                .wrap_err("failed to spawn root shell")?;

            if !status.success() {
                return Err(eyre::eyre!("subshell exited with a non-zero status"));
            }
        }

        Ok(())
    }

    fn delete_local_branch(&self, repo: &Repo) -> color_eyre::Result<()> {
        let git_repo = repo.git();
        match git_repo.find_branch(&self.name, BranchType::Local) {
            Ok(mut branch) => {
                if self.force {
                    drop(branch);
                    let full_ref = format!("refs/heads/{}", self.name);
                    match git_repo.find_reference(&full_ref) {
                        Ok(mut reference) => reference.delete().wrap_err_with(|| {
                            eyre::eyre!("failed to delete local branch reference `{}`", self.name)
                        })?,
                        Err(err) if err.code() == ErrorCode::NotFound => {}
                        Err(err) => {
                            return Err(eyre::eyre!(
                                "failed to look up branch `{}`: {err}",
                                self.name
                            ));
                        }
                    }
                } else {
                    branch.delete().wrap_err_with(|| {
                        eyre::eyre!("failed to delete local branch `{}`", self.name)
                    })?;
                }

                if !self.quiet {
                    let branch_label = format!(
                        "{}",
                        self.name
                            .as_str()
                            .if_supports_color(Stream::Stdout, |text| {
                                format!("{}", text.magenta().bold())
                            })
                    );
                    println!("Deleted local branch `{}`.", branch_label);
                }
            }
            Err(err) if err.code() == ErrorCode::NotFound => {}
            Err(err) => {
                return Err(eyre::eyre!(
                    "failed to look up local branch `{}`: {err}",
                    self.name
                ));
            }
        }

        Ok(())
    }
}

fn find_worktree_name(
    repo: &git2::Repository,
    worktree_path: &Path,
) -> color_eyre::Result<Option<String>> {
    let target = worktree_path
        .canonicalize()
        .unwrap_or_else(|_| worktree_path.to_path_buf());

    let names = repo
        .worktrees()
        .wrap_err("failed to list repository worktrees")?;

    for name in names.iter().flatten() {
        let worktree = match repo.find_worktree(name) {
            Ok(worktree) => worktree,
            Err(err) if err.code() == ErrorCode::NotFound => continue,
            Err(err) => {
                return Err(eyre::eyre!("failed to open git worktree `{name}`: {err}"));
            }
        };

        let path = worktree
            .path()
            .canonicalize()
            .unwrap_or_else(|_| worktree.path().to_path_buf());
        if path == target {
            return Ok(Some(name.to_owned()));
        }
    }

    Ok(None)
}

fn logical_pwd(path: &Path) -> std::ffi::OsString {
    #[cfg(target_os = "macos")]
    {
        if let Ok(stripped) = path.strip_prefix("/private") {
            return Path::new("/").join(stripped).into_os_string();
        }
    }

    path.as_os_str().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    use tempfile::TempDir;

    use crate::{Repo, commands::create::CreateCommand};

    fn init_git_repo(dir: &TempDir) -> color_eyre::Result<()> {
        run(dir, ["git", "init"])?;
        fs::write(dir.path().join("README.md"), "test")?;
        run(dir, ["git", "add", "README.md"])?;
        run(
            dir,
            [
                "git",
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "Initial commit",
            ],
        )?;
        Ok(())
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
            return Err(eyre::eyre!("`{program}` exited with status {status}`"));
        }

        Ok(())
    }

    #[test]
    fn reports_missing_worktree_directory() -> color_eyre::Result<()> {
        let dir = TempDir::new()?;
        init_git_repo(&dir)?;
        let repo = Repo::discover_from(dir.path())?;

        let command = RemoveCommand::new("feature/test".into(), false);
        command.execute(&repo)?;

        Ok(())
    }

    #[test]
    fn removing_current_worktree_repositions_to_root() -> color_eyre::Result<()> {
        let original_dir = match std::env::current_dir() {
            Ok(dir) => dir,
            Err(_) => {
                let fallback = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
                std::env::set_current_dir(&fallback)?;
                fallback
            }
        };
        let dir = TempDir::new()?;
        init_git_repo(&dir)?;
        let repo = Repo::discover_from(dir.path())?;

        unsafe {
            std::env::set_var(SHELL_OVERRIDE_ENV, "env");
        }
        let create = CreateCommand::new("feature/local".into(), None);
        create.execute(&repo)?;

        let worktree_path = repo.worktrees_dir().join("feature/local");
        std::env::set_current_dir(&worktree_path)?;

        let command = RemoveCommand::new("feature/local".into(), false);
        command.execute(&repo)?;

        let new_cwd = std::env::current_dir()?;
        assert_eq!(new_cwd, repo.root());

        std::env::set_current_dir(original_dir)?;
        unsafe {
            std::env::remove_var(SHELL_OVERRIDE_ENV);
        }

        Ok(())
    }

    #[test]
    fn deletes_local_branch_when_requested() -> color_eyre::Result<()> {
        let dir = TempDir::new()?;
        init_git_repo(&dir)?;
        let repo = Repo::discover_from(dir.path())?;

        let create = CreateCommand::new("feature/local".into(), None);
        create.create_without_enter(&repo, true)?;

        let command = RemoveCommand::new("feature/local".into(), false)
            .with_quiet(true)
            .with_remove_local_branch(true);
        command.execute(&repo)?;

        let branch = repo.git().find_branch("feature/local", BranchType::Local);
        assert!(matches!(branch, Err(err) if err.code() == ErrorCode::NotFound));

        Ok(())
    }

    #[test]
    fn keeps_local_branch_when_not_requested() -> color_eyre::Result<()> {
        let dir = TempDir::new()?;
        init_git_repo(&dir)?;
        let repo = Repo::discover_from(dir.path())?;

        let create = CreateCommand::new("feature/local".into(), None);
        create.create_without_enter(&repo, true)?;

        let command = RemoveCommand::new("feature/local".into(), false).with_quiet(true);
        command.execute(&repo)?;

        let branch = repo.git().find_branch("feature/local", BranchType::Local);
        assert!(branch.is_ok(), "expected local branch to remain present");

        Ok(())
    }
}
