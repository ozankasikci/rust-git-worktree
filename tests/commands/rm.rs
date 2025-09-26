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
fn rm_command_removes_existing_worktree() -> Result<(), Box<dyn Error>> {
    let repo_dir = TempDir::new()?;
    init_git_repo(repo_dir.path())?;

    Command::cargo_bin("rsworktree")?
        .current_dir(repo_dir.path())
        .env("RSWORKTREE_SHELL", "env")
        .args(["create", "feature/remove-me"])
        .assert()
        .success();

    let worktree_path = repo_dir
        .path()
        .join(".rsworktree")
        .join("feature/remove-me");
    assert!(worktree_path.exists());

    Command::cargo_bin("rsworktree")?
        .current_dir(repo_dir.path())
        .args(["rm", "feature/remove-me"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Removed worktree"));

    assert!(!worktree_path.exists(), "worktree directory should be gone");

    let list_output = StdCommand::new("git")
        .current_dir(repo_dir.path())
        .args(["worktree", "list", "--porcelain"])
        .output()?;
    assert!(
        !String::from_utf8_lossy(&list_output.stdout).contains(".rsworktree/feature/remove-me")
    );

    Ok(())
}

#[test]
fn rm_command_handles_missing_worktree() -> Result<(), Box<dyn Error>> {
    let repo_dir = TempDir::new()?;
    init_git_repo(repo_dir.path())?;

    Command::cargo_bin("rsworktree")?
        .current_dir(repo_dir.path())
        .args(["rm", "missing"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("does not exist")
                .or(predicate::str::contains("nothing to remove")),
        );

    Ok(())
}

#[test]
fn rm_command_spawns_root_shell_when_called_inside_worktree() -> Result<(), Box<dyn Error>> {
    let repo_dir = TempDir::new()?;
    init_git_repo(repo_dir.path())?;

    Command::cargo_bin("rsworktree")?
        .current_dir(repo_dir.path())
        .env("RSWORKTREE_SHELL", "env")
        .args(["create", "feature/move-back"])
        .assert()
        .success();

    let worktree_path = repo_dir
        .path()
        .join(".rsworktree")
        .join("feature/move-back");
    let repo_root = repo_dir.path().canonicalize()?;

    let repo_root = repo_dir.path();

    Command::cargo_bin("rsworktree")?
        .current_dir(&worktree_path)
        .env("RSWORKTREE_SHELL", "env")
        .args(["rm", "feature/move-back"])
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "PWD={}",
            repo_root.display()
        )));

    Ok(())
}

#[test]
fn rm_command_refuses_locked_worktree_without_force() -> Result<(), Box<dyn Error>> {
    let repo_dir = TempDir::new()?;
    init_git_repo(repo_dir.path())?;

    Command::cargo_bin("rsworktree")?
        .current_dir(repo_dir.path())
        .env("RSWORKTREE_SHELL", "env")
        .args(["create", "feature/locked"])
        .assert()
        .success();

    let worktree_path = repo_dir.path().join(".rsworktree").join("feature/locked");

    let status = StdCommand::new("git")
        .current_dir(repo_dir.path())
        .args(["worktree", "lock", worktree_path.to_str().unwrap()])
        .status()?;
    assert!(status.success(), "git worktree lock should succeed");

    Command::cargo_bin("rsworktree")?
        .current_dir(repo_dir.path())
        .args(["rm", "feature/locked"])
        .assert()
        .failure();

    assert!(worktree_path.exists(), "locked worktree should remain");

    Ok(())
}

#[test]
fn rm_command_force_removes_locked_worktree() -> Result<(), Box<dyn Error>> {
    let repo_dir = TempDir::new()?;
    init_git_repo(repo_dir.path())?;

    Command::cargo_bin("rsworktree")?
        .current_dir(repo_dir.path())
        .env("RSWORKTREE_SHELL", "env")
        .args(["create", "feature/locked-force"])
        .assert()
        .success();

    let worktree_path = repo_dir
        .path()
        .join(".rsworktree")
        .join("feature/locked-force");

    let status = StdCommand::new("git")
        .current_dir(repo_dir.path())
        .args(["worktree", "lock", worktree_path.to_str().unwrap()])
        .status()?;
    assert!(status.success(), "git worktree lock should succeed");

    Command::cargo_bin("rsworktree")?
        .current_dir(repo_dir.path())
        .args(["rm", "feature/locked-force", "--force"])
        .assert()
        .success();

    assert!(
        !worktree_path.exists(),
        "forced removal should delete the worktree directory"
    );

    Ok(())
}
