use std::{fs, process::Command};

use color_eyre::eyre::{self, Context};
use owo_colors::{OwoColorize, Stream};

use crate::Repo;

#[derive(Debug)]
pub struct RemoveCommand {
    name: String,
    force: bool,
}

impl RemoveCommand {
    pub fn new(name: String, force: bool) -> Self {
        Self { name, force }
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
            println!(
                "No worktrees directory found at `{}`; nothing to remove.",
                dir
            );
            return Ok(());
        }

        let worktree_path = worktrees_dir.join(&self.name);
        if !worktree_path.exists() {
            let name = format!(
                "{}",
                self.name
                    .as_str()
                    .if_supports_color(Stream::Stdout, |text| format!("{}", text.cyan()))
            );
            println!(
                "Worktree `{}` does not exist under `{}`.",
                name,
                worktrees_dir.display()
            );
            return Ok(());
        }

        let mut cmd = Command::new("git");
        cmd.current_dir(repo.root());
        cmd.args(["worktree", "remove"]);
        if self.force {
            cmd.arg("--force");
        }
        cmd.arg(&worktree_path);

        let status = cmd
            .status()
            .wrap_err("failed to run `git worktree remove`")?;

        if !status.success() {
            return Err(eyre::eyre!(
                "`git worktree remove` exited with status {status}"
            ));
        }

        if worktree_path.exists() {
            fs::remove_dir_all(&worktree_path).wrap_err_with(|| {
                eyre::eyre!(
                    "failed to clean up worktree directory `{}`",
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
        println!(
            "Removed worktree `{}` from `{}`.",
            name,
            worktrees_dir.display()
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use tempfile::TempDir;

    use crate::Repo;

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
}
