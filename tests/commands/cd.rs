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
fn cd_command_prints_worktree_path() -> Result<(), Box<dyn Error>> {
    let repo_dir = TempDir::new()?;
    init_git_repo(repo_dir.path())?;

    Command::cargo_bin("git-worktree-helper")?
        .current_dir(repo_dir.path())
        .args(["create", "feature/test"])
        .assert()
        .success();

    let worktree_path = repo_dir
        .path()
        .join(".rsworktree")
        .join("feature/test")
        .canonicalize()?;

    Command::cargo_bin("git-worktree-helper")?
        .current_dir(repo_dir.path())
        .args(["cd", "feature/test", "--print"])
        .assert()
        .success()
        .stdout(predicate::str::contains(worktree_path.to_string_lossy()));

    Ok(())
}

#[test]
fn cd_command_spawns_shell_in_worktree() -> Result<(), Box<dyn Error>> {
    let repo_dir = TempDir::new()?;
    init_git_repo(repo_dir.path())?;

    Command::cargo_bin("git-worktree-helper")?
        .current_dir(repo_dir.path())
        .args(["create", "feature/test"])
        .assert()
        .success();

    Command::cargo_bin("git-worktree-helper")?
        .current_dir(repo_dir.path())
        .env("GIT_WORKTREE_HELPER_SHELL", "env")
        .args(["cd", "feature/test"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Spawning shell")
                .and(predicate::str::contains("PWD=/"))
                .and(predicate::str::contains("feature/test")),
        );

    Ok(())
}

#[test]
fn cd_command_fails_for_missing_worktree() -> Result<(), Box<dyn Error>> {
    let repo_dir = TempDir::new()?;
    init_git_repo(repo_dir.path())?;

    Command::cargo_bin("git-worktree-helper")?
        .current_dir(repo_dir.path())
        .args(["cd", "missing", "--print"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not exist"));

    Ok(())
}
