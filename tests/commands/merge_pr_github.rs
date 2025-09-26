#![cfg(unix)]

use std::{
    env, error::Error, ffi::OsString, fs, path::Path, path::PathBuf, process::Command as StdCommand,
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

#[test]
fn merge_pr_github_merges_open_pr_for_current_worktree() -> Result<(), Box<dyn Error>> {
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
        .env(
            "GH_PR_LIST_RESPONSE",
            r#"[{"number": 42, "state": "OPEN"}]"#,
        )
        .args(["merge-pr-github"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Merged PR #42 for branch `feature/test`.",
        ));

    let log_contents = fs::read_to_string(&stub.log_path)?;
    assert!(log_contents.contains("args:pr merge 42 --merge --delete-branch"));

    Ok(())
}

#[test]
fn merge_pr_github_reports_when_no_pr_found() -> Result<(), Box<dyn Error>> {
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
        .env("GH_PR_LIST_RESPONSE", "[]")
        .args(["merge-pr-github"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "No open pull request found for branch `feature/test`.",
        ));

    let log_contents = fs::read_to_string(&stub.log_path)?;
    assert!(log_contents.contains("args:pr list"));
    assert!(!log_contents.contains("args:pr merge"));

    Ok(())
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

fn install_stub_gh() -> Result<StubGh, Box<dyn Error>> {
    let stub_dir = TempDir::new()?;
    let gh_log = stub_dir.path().join("gh.log");
    let gh_path = stub_dir.path().join("gh");
    fs::write(
        &gh_path,
        "#! /bin/sh\n\nlog() {\n  printf 'PWD:%s\\n' \"$PWD\" >> \"$GH_LOG\"\n  printf 'args:%s\\n' \"$*\" >> \"$GH_LOG\"\n}\n\ncase \"$1 $2\" in\n  'pr list')\n    log \"$@\"\n    printf '%s' \"${GH_PR_LIST_RESPONSE:-[]}\"\n    ;;\n  'pr merge')\n    log \"$@\"\n    ;;\n  *)\n    echo \"unexpected gh invocation: $*\" >&2\n    exit 1\n    ;;\nesac\n\nexit 0\n",
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
