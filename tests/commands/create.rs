use std::{collections::HashSet, error::Error, fs, path::Path, process::Command as StdCommand};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn init_git_repo(dir: &Path) -> Result<(), Box<dyn Error>> {
    let init_with_main = StdCommand::new("git")
        .current_dir(dir)
        .args(["init", "-b", "main"])
        .status()?;

    if !init_with_main.success() {
        run(dir, ["git", "init"])?;
        run(dir, ["git", "branch", "-M", "main"])?;
    }

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
fn create_command_creates_worktree_and_updates_gitignore() -> Result<(), Box<dyn Error>> {
    let repo_dir = TempDir::new()?;
    init_git_repo(repo_dir.path())?;

    Command::cargo_bin("rsworktree")?
        .current_dir(repo_dir.path())
        .env("RSWORKTREE_SHELL", "env")
        .args(["create", "feature/test"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created worktree"));

    let worktree_path = repo_dir.path().join(".rsworktree/feature/test");
    assert!(worktree_path.exists(), "worktree directory should exist");

    let gitignore_contents = fs::read_to_string(repo_dir.path().join(".gitignore"))?;
    let occurrences = gitignore_contents
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            trimmed == ".rsworktree/" || trimmed == ".rsworktree"
        })
        .count();
    assert_eq!(occurrences, 1, "`.rsworktree/` should appear exactly once");

    Ok(())
}

#[test]
fn create_command_reuses_existing_branch() -> Result<(), Box<dyn Error>> {
    let repo_dir = TempDir::new()?;
    init_git_repo(repo_dir.path())?;

    run(repo_dir.path(), ["git", "branch", "feature/existing"])?;

    Command::cargo_bin("rsworktree")?
        .current_dir(repo_dir.path())
        .env("RSWORKTREE_SHELL", "env")
        .args(["create", "feature/existing"])
        .assert()
        .success();

    let worktree_path = repo_dir.path().join(".rsworktree").join("feature/existing");
    assert!(worktree_path.exists(), "worktree directory should exist");

    let output = StdCommand::new("git")
        .current_dir(repo_dir.path())
        .args(["worktree", "list", "--porcelain"])
        .output()?;
    assert!(
        String::from_utf8_lossy(&output.stdout).contains(".rsworktree/feature/existing"),
        "git should report the new worktree"
    );

    Ok(())
}

#[test]
fn create_command_accepts_branch_option() -> Result<(), Box<dyn Error>> {
    let repo_dir = TempDir::new()?;
    init_git_repo(repo_dir.path())?;

    Command::cargo_bin("rsworktree")?
        .current_dir(repo_dir.path())
        .env("RSWORKTREE_SHELL", "env")
        .args(["create", "feature/from-main", "--base", "main"])
        .assert()
        .success();

    let feature_rev = StdCommand::new("git")
        .current_dir(repo_dir.path())
        .args(["rev-parse", "feature/from-main"])
        .output()?;
    let main_rev = StdCommand::new("git")
        .current_dir(repo_dir.path())
        .args(["rev-parse", "main"])
        .output()?;

    assert_eq!(feature_rev.stdout, main_rev.stdout);

    Ok(())
}

#[test]
fn create_command_preserves_existing_branch_when_base_specified() -> Result<(), Box<dyn Error>> {
    let repo_dir = TempDir::new()?;
    init_git_repo(repo_dir.path())?;

    run(repo_dir.path(), ["git", "branch", "feature/existing"])?;
    let original_rev = StdCommand::new("git")
        .current_dir(repo_dir.path())
        .args(["rev-parse", "feature/existing"])
        .output()?;

    // Advance main so the branch would move if we recreated it.
    fs::write(repo_dir.path().join("CHANGELOG.md"), "notes")?;
    run(repo_dir.path(), ["git", "add", "."])?;
    run(
        repo_dir.path(),
        [
            "git",
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "Second commit",
        ],
    )?;
    let main_rev_after = StdCommand::new("git")
        .current_dir(repo_dir.path())
        .args(["rev-parse", "main"])
        .output()?;

    Command::cargo_bin("rsworktree")?
        .current_dir(repo_dir.path())
        .env("RSWORKTREE_SHELL", "env")
        .args(["create", "feature/existing", "--base", "main"])
        .assert()
        .success();

    let branch_rev_after = StdCommand::new("git")
        .current_dir(repo_dir.path())
        .args(["rev-parse", "feature/existing"])
        .output()?;

    assert_eq!(branch_rev_after.stdout, original_rev.stdout);
    assert_ne!(branch_rev_after.stdout, main_rev_after.stdout);

    Ok(())
}

#[test]
fn create_command_handles_names_with_reserved_characters() -> Result<(), Box<dyn Error>> {
    let repo_dir = TempDir::new()?;
    init_git_repo(repo_dir.path())?;

    let names = ["feature/a/b", "feature/a-b"];
    for name in names {
        Command::cargo_bin("rsworktree")?
            .current_dir(repo_dir.path())
            .env("RSWORKTREE_SHELL", "env")
            .args(["create", name])
            .assert()
            .success();
    }

    let worktrees_dir = repo_dir.path().join(".rsworktree");
    for name in names {
        assert!(worktrees_dir.join(name).exists());
    }

    let metadata_root = repo_dir.path().join(".git").join("worktrees");
    let entries: HashSet<_> = fs::read_dir(&metadata_root)?
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(entries.len(), 2, "metadata directories should be unique");

    Ok(())
}
