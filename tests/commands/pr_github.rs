#![cfg(unix)]

use std::{
    env,
    error::Error,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command as StdCommand,
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

struct StubGh {
    _dir: TempDir,
    path_value: OsString,
    log_path: PathBuf,
}

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
fn pr_github_reports_missing_worktree() -> Result<(), Box<dyn Error>> {
    let repo_dir = TempDir::new()?;
    init_git_repo(repo_dir.path())?;

    Command::cargo_bin("rsworktree")?
        .current_dir(repo_dir.path())
        .args(["pr-github", "missing", "--no-push"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not exist"));

    Ok(())
}

#[test]
fn pr_github_invokes_gh_with_expected_arguments() -> Result<(), Box<dyn Error>> {
    let repo_dir = TempDir::new()?;
    init_git_repo(repo_dir.path())?;

    Command::cargo_bin("rsworktree")?
        .current_dir(repo_dir.path())
        .env("RSWORKTREE_SHELL", "env")
        .args(["create", "feature/test"])
        .assert()
        .success();

    let stub = install_stub_gh()?;

    Command::cargo_bin("rsworktree")?
        .current_dir(repo_dir.path())
        .env("PATH", &stub.path_value)
        .env("GH_LOG", &stub.log_path)
        .args([
            "pr-github",
            "feature/test",
            "--no-push",
            "--draft",
            "--fill",
            "--reviewer",
            "octocat",
            "--",
            "--label",
            "ready",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("GitHub pull request created"));

    let log_contents = fs::read_to_string(&stub.log_path)?;
    let worktree_path = repo_dir
        .path()
        .join(".rsworktree")
        .join("feature/test")
        .canonicalize()?;
    assert!(log_contents.contains(worktree_path.to_string_lossy().as_ref()));
    assert!(log_contents.contains(
        "args:pr create --head feature/test --draft --fill --reviewer octocat --label ready"
    ));

    Ok(())
}

#[test]
fn pr_github_defaults_to_current_worktree() -> Result<(), Box<dyn Error>> {
    let repo_dir = TempDir::new()?;
    init_git_repo(repo_dir.path())?;

    Command::cargo_bin("rsworktree")?
        .current_dir(repo_dir.path())
        .env("RSWORKTREE_SHELL", "env")
        .args(["create", "feature/test"])
        .assert()
        .success();

    let worktree_path = repo_dir
        .path()
        .join(".rsworktree")
        .join("feature/test")
        .canonicalize()?;

    let stub = install_stub_gh()?;

    Command::cargo_bin("rsworktree")?
        .current_dir(&worktree_path)
        .env("PATH", &stub.path_value)
        .env("GH_LOG", &stub.log_path)
        .args(["pr-github", "--no-push", "--fill", "--", "--label", "ready"])
        .assert()
        .success();

    let log_contents = fs::read_to_string(&stub.log_path)?;
    let worktree_display = worktree_path.to_string_lossy().into_owned();
    assert!(log_contents.contains(&worktree_display));
    assert!(log_contents.contains("args:pr create --head feature/test --fill --label ready"));

    Ok(())
}

#[test]
fn pr_github_without_name_errors_outside_worktree() -> Result<(), Box<dyn Error>> {
    let repo_dir = TempDir::new()?;
    init_git_repo(repo_dir.path())?;

    Command::cargo_bin("rsworktree")?
        .current_dir(repo_dir.path())
        .args(["pr-github"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("must be run from inside"));

    Ok(())
}

#[test]
fn pr_github_defaults_to_fill_when_metadata_missing() -> Result<(), Box<dyn Error>> {
    let repo_dir = TempDir::new()?;
    init_git_repo(repo_dir.path())?;

    Command::cargo_bin("rsworktree")?
        .current_dir(repo_dir.path())
        .env("RSWORKTREE_SHELL", "env")
        .args(["create", "feature/test"])
        .assert()
        .success();

    let worktree_path = repo_dir
        .path()
        .join(".rsworktree")
        .join("feature/test")
        .canonicalize()?;

    let stub = install_stub_gh()?;

    Command::cargo_bin("rsworktree")?
        .current_dir(&worktree_path)
        .env("PATH", &stub.path_value)
        .env("GH_LOG", &stub.log_path)
        .args(["pr-github", "--no-push"])
        .assert()
        .success();

    let log_contents = fs::read_to_string(&stub.log_path)?;
    assert!(log_contents.contains("args:pr create --head feature/test --fill"));

    Ok(())
}

fn install_stub_gh() -> Result<StubGh, Box<dyn Error>> {
    let stub_dir = TempDir::new()?;
    let gh_log = stub_dir.path().join("gh.log");
    let gh_path = stub_dir.path().join("gh");
    fs::write(
        &gh_path,
        "#! /bin/sh\n\nprintf '%s\n' \"$PWD\" > \"$GH_LOG\"\nprintf 'args:%s\n' \"$*\" >> \"$GH_LOG\"\n",
    )?;
    #[cfg(unix)]
    {
        let mut perms = fs::metadata(&gh_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&gh_path, perms)?;
    }

    let mut path_value = OsString::from(stub_dir.path());
    if let Some(existing) = env::var_os("PATH") {
        path_value.push(":");
        path_value.push(existing);
    }

    Ok(StubGh {
        _dir: stub_dir,
        path_value,
        log_path: gh_log,
    })
}
