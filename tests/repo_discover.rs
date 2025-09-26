use std::{error::Error, fs, path::Path, process::Command as StdCommand};

use assert_cmd::Command;
use rsworktree::Repo;
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
fn repo_discover_from_inside_linked_worktree_finds_parent_repo() -> Result<(), Box<dyn Error>> {
    let repo_dir = TempDir::new()?;
    init_git_repo(repo_dir.path())?;

    Command::cargo_bin("rsworktree")?
        .current_dir(repo_dir.path())
        .env("RSWORKTREE_SHELL", "env")
        .args(["create", "feature/discover"])
        .assert()
        .success();

    let worktree_path = repo_dir.path().join(".rsworktree").join("feature/discover");
    assert!(worktree_path.exists(), "worktree should exist");

    let repo_from_root = Repo::discover_from(&worktree_path)?;
    let expected_root = fs::canonicalize(repo_dir.path())?;
    let actual_root = fs::canonicalize(repo_from_root.root())?;
    assert_eq!(actual_root, expected_root);

    let expected_git_dir = repo_dir.path().join(".git").canonicalize()?;
    let actual_git_dir = fs::canonicalize(repo_from_root.git().path())?;
    assert_eq!(actual_git_dir, expected_git_dir);

    let nested_path = worktree_path.join("nested/subdir");
    fs::create_dir_all(&nested_path)?;

    let repo_from_nested = Repo::discover_from(&nested_path)?;
    let nested_root = fs::canonicalize(repo_from_nested.root())?;
    assert_eq!(nested_root, expected_root);
    let nested_git_dir = fs::canonicalize(repo_from_nested.git().path())?;
    assert_eq!(nested_git_dir, expected_git_dir);

    Ok(())
}
