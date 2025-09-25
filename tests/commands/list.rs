use std::{error::Error, fs, path::Path, process::Command as StdCommand};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn init_git_repo(dir: &Path) -> Result<(), Box<dyn Error>> {
    run(dir, ["git", "init"])?;
    fs::write(dir.join("README.md"), "test")?;
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

fn run(dir: &Path, cmd: impl IntoIterator<Item = &'static str>) -> Result<(), Box<dyn Error>> {
    let mut iter = cmd.into_iter();
    let program = iter.next().expect("command must not be empty");
    let status = StdCommand::new(program)
        .current_dir(dir)
        .args(iter)
        .status()?;

    if !status.success() {
        return Err(format!("`{program}` exited with status {status}").into());
    }

    Ok(())
}

#[test]
fn ls_command_lists_created_worktrees() -> Result<(), Box<dyn Error>> {
    let repo_dir = TempDir::new()?;
    init_git_repo(repo_dir.path())?;

    for name in ["feature/test", "bugfix/fix"] {
        Command::cargo_bin("rsworktree")?
            .current_dir(repo_dir.path())
            .env("GIT_WORKTREE_HELPER_SHELL", "env")
            .args(["create", name])
            .assert()
            .success();
    }

    Command::cargo_bin("rsworktree")?
        .current_dir(repo_dir.path())
        .arg("ls")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("bugfix/fix")
                .and(predicate::str::contains("feature/test"))
                .and(predicate::str::contains("Worktrees under")),
        );

    Ok(())
}

#[test]
fn ls_command_shows_none_when_empty() -> Result<(), Box<dyn Error>> {
    let repo_dir = TempDir::new()?;
    init_git_repo(repo_dir.path())?;

    Command::cargo_bin("rsworktree")?
        .current_dir(repo_dir.path())
        .arg("ls")
        .assert()
        .success()
        .stdout(predicate::str::contains("(none)"));

    Ok(())
}
