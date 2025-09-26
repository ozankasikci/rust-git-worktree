use std::{fs, process::Command};

use color_eyre::eyre::{self, Context};

use owo_colors::{OwoColorize, Stream};

use crate::{Repo, commands::cd::CdCommand};

#[derive(Debug)]
pub struct CreateCommand {
    name: String,
    base: Option<String>,
}

impl CreateCommand {
    pub fn new(name: String, base: Option<String>) -> Self {
        Self { name, base }
    }

    pub fn execute(&self, repo: &Repo) -> color_eyre::Result<()> {
        let worktrees_dir = repo.ensure_worktrees_dir()?;
        let worktree_path = worktrees_dir.join(&self.name);
        let target_branch = self.name.as_str();
        let base_branch = self.base.as_deref();

        if worktree_path.exists() {
            let name = format!(
                "{}",
                self.name
                    .as_str()
                    .if_supports_color(Stream::Stdout, |text| {
                        format!("{}", text.cyan().bold())
                    })
            );
            println!(
                "Worktree `{}` already exists at `{}`.",
                name,
                worktree_path.display()
            );
            return self.enter_worktree(repo);
        }

        if let Some(parent) = worktree_path.parent() {
            fs::create_dir_all(parent).wrap_err_with(|| {
                eyre::eyre!("failed to prepare directory `{}`", parent.display())
            })?;
        }

        let branch_exists = branch_exists(repo, target_branch)?;

        let mut cmd = Command::new("git");
        cmd.current_dir(repo.root());
        cmd.args(["worktree", "add"]);
        cmd.arg(&worktree_path);

        if branch_exists {
            cmd.arg(target_branch);
        } else {
            cmd.args(["-b", target_branch]);
            if let Some(base) = base_branch {
                cmd.arg(base);
            }
        }

        let status = cmd.status().wrap_err("failed to run `git worktree add`")?;

        if !status.success() {
            return Err(eyre::eyre!(
                "`git worktree add` exited with status {status}"
            ));
        }

        let name = format!(
            "{}",
            target_branch.if_supports_color(Stream::Stdout, |text| {
                format!("{}", text.green().bold())
            })
        );
        let path_raw = format!("{}", worktree_path.display());
        let path = format!(
            "{}",
            path_raw
                .as_str()
                .if_supports_color(Stream::Stdout, |text| { format!("{}", text.blue()) })
        );
        if let Some(base) = base_branch {
            let base = format!(
                "{}",
                base.if_supports_color(Stream::Stdout, |text| {
                    format!("{}", text.magenta().bold())
                })
            );
            println!("Created worktree `{}` at `{}` from `{}`.", name, path, base);
        } else {
            println!("Created worktree `{}` at `{}`.", name, path);
        }

        self.enter_worktree(repo)
    }

    fn enter_worktree(&self, repo: &Repo) -> color_eyre::Result<()> {
        CdCommand::new(self.name.clone(), false).execute(repo)
    }
}

fn branch_exists(repo: &Repo, branch: &str) -> color_eyre::Result<bool> {
    let full_ref = format!("refs/heads/{branch}");
    let status = Command::new("git")
        .current_dir(repo.root())
        .args(["show-ref", "--verify", "--quiet", &full_ref])
        .status()
        .wrap_err("failed to run `git show-ref`")?;

    match status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(eyre::eyre!(
            "`git show-ref` exited with unexpected status {status}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use tempfile::TempDir;

    use crate::{Repo, commands::cd::SHELL_OVERRIDE_ENV};

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
            return Err(eyre::eyre!("`{program}` exited with status {status}"));
        }

        Ok(())
    }

    #[test]
    fn creates_new_worktree_under_rsworktree_directory() -> color_eyre::Result<()> {
        let dir = TempDir::new()?;
        init_git_repo(&dir)?;

        let repo = Repo::discover_from(dir.path())?;
        unsafe {
            std::env::set_var(SHELL_OVERRIDE_ENV, "env");
        }
        let command = CreateCommand::new("feature/test".into(), None);
        command.execute(&repo)?;

        let expected_dir = repo.worktrees_dir().join("feature/test");
        assert!(
            expected_dir.exists(),
            "worktree directory should be created"
        );

        command.execute(&repo)?;

        let gitignore_path = repo.root().join(".gitignore");
        let gitignore_contents = fs::read_to_string(&gitignore_path)?;
        let occurrences = gitignore_contents
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                trimmed == ".rsworktree/" || trimmed == ".rsworktree"
            })
            .count();
        assert_eq!(
            occurrences, 1,
            "`.rsworktree/` entry should be present once"
        );

        Ok(())
    }
}
