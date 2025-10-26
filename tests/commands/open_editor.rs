use std::{env, error::Error, fs, path::Path, process::Command as StdCommand};

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

fn create_worktree(repo_dir: &Path, name: &str) -> Result<(), Box<dyn Error>> {
    Command::cargo_bin("rsworktree")?
        .current_dir(repo_dir)
        .env("RSWORKTREE_SHELL", "env")
        .args(["create", name])
        .assert()
        .success();
    Ok(())
}

#[test]
fn open_editor_uses_env_editor_and_reports_success() -> Result<(), Box<dyn Error>> {
    let repo_dir = TempDir::new()?;
    init_git_repo(repo_dir.path())?;
    create_worktree(repo_dir.path(), "feature/test")?;

    let editor_cmd = "/usr/bin/env true";
    let guard = EnvGuard::set("EDITOR", editor_cmd);

    Command::cargo_bin("rsworktree")?
        .current_dir(repo_dir.path())
        .args(["worktree", "open-editor", "feature/test"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Opened `feature/test`").and(predicate::str::contains(
                "Launched `feature/test` using `/usr/bin/env`",
            )),
        );

    drop(guard);
    Ok(())
}

#[test]
fn open_editor_guidance_when_no_preference() -> Result<(), Box<dyn Error>> {
    let repo_dir = TempDir::new()?;
    init_git_repo(repo_dir.path())?;
    create_worktree(repo_dir.path(), "feature/empty")?;

    let guard_editor = EnvGuard::remove("EDITOR");
    let guard_visual = EnvGuard::remove("VISUAL");

    Command::cargo_bin("rsworktree")?
        .current_dir(repo_dir.path())
        .args(["worktree", "open-editor", "feature/empty"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No editor configured"));

    drop(guard_visual);
    drop(guard_editor);
    Ok(())
}

#[test]
fn open_editor_errors_when_worktree_missing() -> Result<(), Box<dyn Error>> {
    let repo_dir = TempDir::new()?;
    init_git_repo(repo_dir.path())?;

    Command::cargo_bin("rsworktree")?
        .current_dir(repo_dir.path())
        .args(["worktree", "open-editor", "missing"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("worktree `missing` not found"));

    Ok(())
}

struct EnvGuard {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = env::var_os(key);
        unsafe {
            env::set_var(key, value);
        }
        Self { key, previous }
    }

    fn remove(key: &'static str) -> Self {
        let previous = env::var_os(key);
        unsafe {
            env::remove_var(key);
        }
        Self { key, previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        if let Some(value) = self.previous.take() {
            unsafe {
                env::set_var(self.key, value);
            }
        } else {
            unsafe {
                env::remove_var(self.key);
            }
        }
    }
}
