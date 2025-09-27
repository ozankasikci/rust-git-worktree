use std::path::{Path, PathBuf};

use color_eyre::eyre::{self, WrapErr};
use owo_colors::{OwoColorize, Stream};
use serde::Deserialize;

use crate::{
    Repo,
    commands::pr_github::{CommandOutput, CommandRunner, SystemCommandRunner},
};

#[derive(Debug)]
pub struct MergePrGithubCommand<R = SystemCommandRunner> {
    name: String,
    remove_local_branch: bool,
    remove_remote_branch: bool,
    runner: R,
}

impl MergePrGithubCommand {
    pub fn new(name: String) -> Self {
        Self::with_runner(name, SystemCommandRunner)
    }
}

impl<R> MergePrGithubCommand<R>
where
    R: CommandRunner,
{
    pub fn with_runner(name: String, runner: R) -> Self {
        Self {
            name,
            remove_local_branch: true,
            remove_remote_branch: false,
            runner,
        }
    }

    pub fn disable_remove_local(&mut self) {
        self.remove_local_branch = false;
    }

    pub fn enable_remove_remote(&mut self) {
        self.remove_remote_branch = true;
    }

    pub fn execute(&mut self, repo: &Repo) -> color_eyre::Result<()> {
        let worktree_path = self.ensure_worktree_path(repo)?;
        let branch = self.determine_branch(&worktree_path)?;
        let repo_root = repo.root().to_path_buf();

        let branch_label = format_with_color(&branch, |text| format!("{}", text.magenta().bold()));
        let path_label = format_with_color(&worktree_path.display().to_string(), |text| {
            format!("{}", text.blue())
        });
        println!(
            "Looking for open PR for `{}` from `{}`...",
            branch_label, path_label
        );

        match self.find_pull_request(&repo_root, &branch)? {
            Some(pr_number) => {
                self.merge_pull_request(&repo_root, &branch, &worktree_path, pr_number)
            }
            None => {
                println!("No open pull request found for branch `{}`.", branch_label);
                Ok(())
            }
        }
    }

    fn ensure_worktree_path(&self, repo: &Repo) -> color_eyre::Result<PathBuf> {
        let worktrees_dir = repo.ensure_worktrees_dir()?;
        let worktree_path = worktrees_dir.join(&self.name);
        if !worktree_path.exists() {
            return Err(eyre::eyre!(
                "worktree `{}` does not exist under `{}`",
                self.name,
                worktrees_dir.display()
            ));
        }
        Ok(worktree_path)
    }

    fn determine_branch(&mut self, worktree_path: &Path) -> color_eyre::Result<String> {
        let args = vec![
            "rev-parse".to_owned(),
            "--abbrev-ref".to_owned(),
            "HEAD".to_owned(),
        ];
        let output = self
            .runner
            .run("git", worktree_path, &args)
            .wrap_err("failed to determine current branch with `git rev-parse`")?;

        if !output.success {
            return Err(command_failure("git", &args, &output));
        }

        let branch = output.stdout.trim();
        if branch.is_empty() {
            return Err(eyre::eyre!("`git rev-parse` produced empty branch name"));
        }

        Ok(branch.to_owned())
    }

    fn find_pull_request(
        &mut self,
        repo_path: &Path,
        branch: &str,
    ) -> color_eyre::Result<Option<u64>> {
        let args = vec![
            "pr".to_owned(),
            "list".to_owned(),
            "--head".to_owned(),
            branch.to_owned(),
            "--state".to_owned(),
            "open".to_owned(),
            "--json".to_owned(),
            "number".to_owned(),
            "--limit".to_owned(),
            "1".to_owned(),
        ];

        let output = self
            .runner
            .run("gh", repo_path, &args)
            .wrap_err("failed to run `gh pr list`")?;

        if !output.success {
            return Err(command_failure("gh", &args, &output));
        }

        let stdout = output.stdout.trim();
        if stdout.is_empty() {
            return Ok(None);
        }

        let prs: Vec<PullRequestInfo> =
            serde_json::from_str(stdout).wrap_err("failed to parse `gh pr list` output as JSON")?;

        Ok(prs.into_iter().next().map(|pr| pr.number))
    }

    fn merge_pull_request(
        &mut self,
        repo_path: &Path,
        branch: &str,
        worktree_path: &Path,
        pr_number: u64,
    ) -> color_eyre::Result<()> {
        let mut args = vec![
            "pr".to_owned(),
            "merge".to_owned(),
            pr_number.to_string(),
            "--merge".to_owned(),
        ];
        if self.remove_local_branch {
            args.push("--delete-branch".to_owned());
        }

        let output = self
            .runner
            .run("gh", repo_path, &args)
            .wrap_err("failed to run `gh pr merge`")?;

        let branch_delete_failed = self.remove_local_branch && gh_branch_delete_failure(&output);

        if !output.success && !branch_delete_failed {
            return Err(command_failure("gh", &args, &output));
        }

        let pr_label = format_with_color(&format!("#{}", pr_number), |text| {
            format!("{}", text.green().bold())
        });
        let branch_label = format_with_color(branch, |text| format!("{}", text.magenta().bold()));

        if branch_delete_failed {
            let warning = format!(
                "PR {} merged but `gh` could not delete branch `{}`. Leaving the branch intact.",
                pr_label, branch_label
            );
            println!(
                "{}",
                warning.if_supports_color(Stream::Stdout, |text| format!("{}", text.yellow()))
            );
        }

        self.restore_worktree_branch(worktree_path, branch)?;

        if self.remove_remote_branch {
            self.delete_remote_branch(repo_path, branch)?;
        }
        println!("Merged PR {} for branch `{}`.", pr_label, branch_label);
        Ok(())
    }

    fn restore_worktree_branch(
        &mut self,
        worktree_path: &Path,
        branch: &str,
    ) -> color_eyre::Result<()> {
        let args = vec!["switch".to_owned(), branch.to_owned()];
        let output = self
            .runner
            .run("git", worktree_path, &args)
            .wrap_err("failed to restore worktree branch with `git switch`")?;

        if !output.success {
            return Err(command_failure("git", &args, &output));
        }

        Ok(())
    }

    fn delete_remote_branch(&mut self, repo_path: &Path, branch: &str) -> color_eyre::Result<()> {
        let args = vec![
            "push".to_owned(),
            "origin".to_owned(),
            "--delete".to_owned(),
            branch.to_owned(),
        ];

        let output = self
            .runner
            .run("git", repo_path, &args)
            .wrap_err("failed to delete remote branch with `git push`")?;

        let branch_label = format_with_color(branch, |text| format!("{}", text.magenta().bold()));

        if !output.success {
            if remote_branch_already_gone(&output) {
                println!("Remote branch `{}` was already removed.", branch_label);
                return Ok(());
            }
            return Err(command_failure("git", &args, &output));
        }

        println!("Removed remote branch `{}`.", branch_label);
        Ok(())
    }
}

fn gh_branch_delete_failure(output: &CommandOutput) -> bool {
    if output.success {
        return false;
    }

    let stderr = output.stderr.to_lowercase();
    stderr.contains("failed to delete local branch") || stderr.contains("cannot delete branch")
}

fn remote_branch_already_gone(output: &CommandOutput) -> bool {
    if output.success {
        return false;
    }

    let combined = format!("{}{}", output.stderr, output.stdout).to_lowercase();
    combined.contains("remote ref does not exist")
}

fn command_failure(program: &str, args: &[String], output: &CommandOutput) -> color_eyre::Report {
    let command_line = format_command(program, args);
    let status = match output.status_code {
        Some(code) => format!("exit status {code}"),
        None => "termination by signal".to_owned(),
    };

    let mut message = format!("`{command_line}` failed with {status}");
    let stderr = output.stderr.trim();
    if !stderr.is_empty() {
        message.push('\n');
        message.push_str(stderr);
    }

    eyre::eyre!(message)
}

fn format_command(program: &str, args: &[String]) -> String {
    let mut parts = Vec::with_capacity(1 + args.len());
    parts.push(quote_arg(program));
    for arg in args {
        parts.push(quote_arg(arg));
    }
    parts.join(" ")
}

fn quote_arg(value: &str) -> String {
    if value
        .chars()
        .all(|c| matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '/' | '='))
    {
        value.to_owned()
    } else {
        let escaped = value.replace('\'', "'\\''");
        format!("'{escaped}'")
    }
}

fn format_with_color(value: &str, paint: impl Fn(&str) -> String) -> String {
    value
        .if_supports_color(Stream::Stdout, |text| paint(text))
        .to_string()
}

#[derive(Debug, Deserialize)]
struct PullRequestInfo {
    number: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::VecDeque, fs};

    use tempfile::TempDir;

    use crate::Repo;

    #[derive(Debug, Default)]
    struct MockCommandRunner {
        responses: VecDeque<color_eyre::Result<CommandOutput>>,
        calls: Vec<RecordedCall>,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct RecordedCall {
        program: String,
        dir: PathBuf,
        args: Vec<String>,
    }

    impl CommandRunner for MockCommandRunner {
        fn run(
            &mut self,
            program: &str,
            current_dir: &Path,
            args: &[String],
        ) -> color_eyre::Result<CommandOutput> {
            self.calls.push(RecordedCall {
                program: program.to_owned(),
                dir: current_dir.to_path_buf(),
                args: args.to_vec(),
            });
            self.responses
                .pop_front()
                .unwrap_or_else(|| Err(eyre::eyre!("unexpected command invocation")))
        }
    }

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
        let status = std::process::Command::new(program)
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
    fn merges_when_pull_request_found() -> color_eyre::Result<()> {
        let repo_dir = TempDir::new()?;
        init_git_repo(&repo_dir)?;
        let repo = Repo::discover_from(repo_dir.path())?;
        let repo_root = repo.root().to_path_buf();
        let worktree_path = repo.worktrees_dir().join("feature/test");
        fs::create_dir_all(&worktree_path)?;

        let mut runner = MockCommandRunner::default();
        runner.responses.extend([
            Ok(CommandOutput {
                stdout: "feature/test\n".into(),
                stderr: String::new(),
                success: true,
                status_code: Some(0),
            }),
            Ok(CommandOutput {
                stdout: "[{\"number\":42}]".into(),
                stderr: String::new(),
                success: true,
                status_code: Some(0),
            }),
            Ok(CommandOutput {
                stdout: String::new(),
                stderr: String::new(),
                success: true,
                status_code: Some(0),
            }),
            Ok(CommandOutput {
                stdout: String::new(),
                stderr: String::new(),
                success: true,
                status_code: Some(0),
            }),
        ]);

        let mut command = MergePrGithubCommand::with_runner("feature/test".into(), runner);
        command.execute(&repo)?;

        assert_eq!(
            command.runner.calls,
            vec![
                RecordedCall {
                    program: "git".into(),
                    dir: worktree_path.clone(),
                    args: vec!["rev-parse".into(), "--abbrev-ref".into(), "HEAD".into()],
                },
                RecordedCall {
                    program: "gh".into(),
                    dir: repo_root.clone(),
                    args: vec![
                        "pr".into(),
                        "list".into(),
                        "--head".into(),
                        "feature/test".into(),
                        "--state".into(),
                        "open".into(),
                        "--json".into(),
                        "number".into(),
                        "--limit".into(),
                        "1".into(),
                    ],
                },
                RecordedCall {
                    program: "gh".into(),
                    dir: repo_root,
                    args: vec![
                        "pr".into(),
                        "merge".into(),
                        "42".into(),
                        "--merge".into(),
                        "--delete-branch".into(),
                    ],
                },
                RecordedCall {
                    program: "git".into(),
                    dir: worktree_path.clone(),
                    args: vec!["switch".into(), "feature/test".into()],
                },
            ]
        );

        Ok(())
    }

    #[test]
    fn removes_remote_branch_when_requested() -> color_eyre::Result<()> {
        let repo_dir = TempDir::new()?;
        init_git_repo(&repo_dir)?;
        let repo = Repo::discover_from(repo_dir.path())?;
        let repo_root = repo.root().to_path_buf();
        let worktree_path = repo.worktrees_dir().join("feature/remove");
        fs::create_dir_all(&worktree_path)?;

        let mut runner = MockCommandRunner::default();
        runner.responses.extend([
            Ok(CommandOutput {
                stdout: "feature/remove\n".into(),
                stderr: String::new(),
                success: true,
                status_code: Some(0),
            }),
            Ok(CommandOutput {
                stdout: "[{\"number\":99}]".into(),
                stderr: String::new(),
                success: true,
                status_code: Some(0),
            }),
            Ok(CommandOutput {
                stdout: String::new(),
                stderr: String::new(),
                success: true,
                status_code: Some(0),
            }),
            Ok(CommandOutput {
                stdout: String::new(),
                stderr: String::new(),
                success: true,
                status_code: Some(0),
            }),
            Ok(CommandOutput {
                stdout: String::new(),
                stderr: String::new(),
                success: true,
                status_code: Some(0),
            }),
        ]);

        let mut command = MergePrGithubCommand::with_runner("feature/remove".into(), runner);
        command.enable_remove_remote();
        command.execute(&repo)?;

        assert_eq!(
            command.runner.calls,
            vec![
                RecordedCall {
                    program: "git".into(),
                    dir: worktree_path.clone(),
                    args: vec!["rev-parse".into(), "--abbrev-ref".into(), "HEAD".into()],
                },
                RecordedCall {
                    program: "gh".into(),
                    dir: repo_root.clone(),
                    args: vec![
                        "pr".into(),
                        "list".into(),
                        "--head".into(),
                        "feature/remove".into(),
                        "--state".into(),
                        "open".into(),
                        "--json".into(),
                        "number".into(),
                        "--limit".into(),
                        "1".into(),
                    ],
                },
                RecordedCall {
                    program: "gh".into(),
                    dir: repo_root.clone(),
                    args: vec![
                        "pr".into(),
                        "merge".into(),
                        "99".into(),
                        "--merge".into(),
                        "--delete-branch".into(),
                    ],
                },
                RecordedCall {
                    program: "git".into(),
                    dir: worktree_path.clone(),
                    args: vec!["switch".into(), "feature/remove".into()],
                },
                RecordedCall {
                    program: "git".into(),
                    dir: repo_root,
                    args: vec![
                        "push".into(),
                        "origin".into(),
                        "--delete".into(),
                        "feature/remove".into(),
                    ],
                },
            ]
        );

        Ok(())
    }

    #[test]
    fn keeps_local_branch_when_disabled() -> color_eyre::Result<()> {
        let repo_dir = TempDir::new()?;
        init_git_repo(&repo_dir)?;
        let repo = Repo::discover_from(repo_dir.path())?;
        let repo_root = repo.root().to_path_buf();
        let worktree_path = repo.worktrees_dir().join("feature/keep-local");
        fs::create_dir_all(&worktree_path)?;

        let mut runner = MockCommandRunner::default();
        runner.responses.extend([
            Ok(CommandOutput {
                stdout: "feature/keep-local\n".into(),
                stderr: String::new(),
                success: true,
                status_code: Some(0),
            }),
            Ok(CommandOutput {
                stdout: "[{\"number\":123}]".into(),
                stderr: String::new(),
                success: true,
                status_code: Some(0),
            }),
            Ok(CommandOutput {
                stdout: String::new(),
                stderr: String::new(),
                success: true,
                status_code: Some(0),
            }),
            Ok(CommandOutput {
                stdout: String::new(),
                stderr: String::new(),
                success: true,
                status_code: Some(0),
            }),
        ]);

        let mut command = MergePrGithubCommand::with_runner("feature/keep-local".into(), runner);
        command.disable_remove_local();
        command.execute(&repo)?;

        assert_eq!(
            command.runner.calls,
            vec![
                RecordedCall {
                    program: "git".into(),
                    dir: worktree_path.clone(),
                    args: vec!["rev-parse".into(), "--abbrev-ref".into(), "HEAD".into()],
                },
                RecordedCall {
                    program: "gh".into(),
                    dir: repo_root.clone(),
                    args: vec![
                        "pr".into(),
                        "list".into(),
                        "--head".into(),
                        "feature/keep-local".into(),
                        "--state".into(),
                        "open".into(),
                        "--json".into(),
                        "number".into(),
                        "--limit".into(),
                        "1".into(),
                    ],
                },
                RecordedCall {
                    program: "gh".into(),
                    dir: repo_root,
                    args: vec!["pr".into(), "merge".into(), "123".into(), "--merge".into()],
                },
                RecordedCall {
                    program: "git".into(),
                    dir: worktree_path,
                    args: vec!["switch".into(), "feature/keep-local".into()],
                },
            ]
        );

        Ok(())
    }

    #[test]
    fn treat_missing_remote_branch_as_success() -> color_eyre::Result<()> {
        let repo_dir = TempDir::new()?;
        init_git_repo(&repo_dir)?;
        let repo = Repo::discover_from(repo_dir.path())?;
        let worktree_path = repo.worktrees_dir().join("feature/missing");
        fs::create_dir_all(&worktree_path)?;

        let mut runner = MockCommandRunner::default();
        runner.responses.extend([
            Ok(CommandOutput {
                stdout: "feature/missing\n".into(),
                stderr: String::new(),
                success: true,
                status_code: Some(0),
            }),
            Ok(CommandOutput {
                stdout: "[{\"number\":7}]".into(),
                stderr: String::new(),
                success: true,
                status_code: Some(0),
            }),
            Ok(CommandOutput {
                stdout: String::new(),
                stderr: String::new(),
                success: true,
                status_code: Some(0),
            }),
            Ok(CommandOutput {
                stdout: String::new(),
                stderr: String::new(),
                success: true,
                status_code: Some(0),
            }),
            Ok(CommandOutput {
                stdout:
                    "To origin\n - [deleted] feature/missing\nerror: failed to push some refs\n"
                        .into(),
                stderr: "error: unable to delete 'feature/missing': remote ref does not exist\n"
                    .into(),
                success: false,
                status_code: Some(1),
            }),
        ]);

        let mut command = MergePrGithubCommand::with_runner("feature/missing".into(), runner);
        command.enable_remove_remote();
        command.execute(&repo)?;

        assert_eq!(command.runner.calls.len(), 5);

        Ok(())
    }

    #[test]
    fn surface_remote_branch_deletion_failures() -> color_eyre::Result<()> {
        let repo_dir = TempDir::new()?;
        init_git_repo(&repo_dir)?;
        let repo = Repo::discover_from(repo_dir.path())?;
        let worktree_path = repo.worktrees_dir().join("feature/error");
        fs::create_dir_all(&worktree_path)?;

        let mut runner = MockCommandRunner::default();
        runner.responses.extend([
            Ok(CommandOutput {
                stdout: "feature/error\n".into(),
                stderr: String::new(),
                success: true,
                status_code: Some(0),
            }),
            Ok(CommandOutput {
                stdout: "[{\"number\":13}]".into(),
                stderr: String::new(),
                success: true,
                status_code: Some(0),
            }),
            Ok(CommandOutput {
                stdout: String::new(),
                stderr: String::new(),
                success: true,
                status_code: Some(0),
            }),
            Ok(CommandOutput {
                stdout: String::new(),
                stderr: String::new(),
                success: true,
                status_code: Some(0),
            }),
            Ok(CommandOutput {
                stdout: String::new(),
                stderr: "error: unable to delete branch due to permissions\n".into(),
                success: false,
                status_code: Some(1),
            }),
        ]);

        let mut command = MergePrGithubCommand::with_runner("feature/error".into(), runner);
        command.enable_remove_remote();
        let result = command.execute(&repo);
        assert!(
            result.is_err(),
            "expected deletion failure to surface as error"
        );

        Ok(())
    }

    #[test]
    fn treats_branch_delete_failure_as_success() -> color_eyre::Result<()> {
        let repo_dir = TempDir::new()?;
        init_git_repo(&repo_dir)?;
        let repo = Repo::discover_from(repo_dir.path())?;
        let repo_root = repo.root().to_path_buf();
        let worktree_path = repo.worktrees_dir().join("feature/test");
        fs::create_dir_all(&worktree_path)?;

        let mut runner = MockCommandRunner::default();
        runner.responses.extend([
            Ok(CommandOutput {
                stdout: "feature/test\n".into(),
                stderr: String::new(),
                success: true,
                status_code: Some(0),
            }),
            Ok(CommandOutput {
                stdout: "[{\"number\":42}]".into(),
                stderr: String::new(),
                success: true,
                status_code: Some(0),
            }),
            Ok(CommandOutput {
                stdout: "Pull request successfully merged".into(),
                stderr: "failed to delete local branch merge-cmd".into(),
                success: false,
                status_code: Some(1),
            }),
            Ok(CommandOutput {
                stdout: String::new(),
                stderr: String::new(),
                success: true,
                status_code: Some(0),
            }),
        ]);

        let mut command = MergePrGithubCommand::with_runner("feature/test".into(), runner);
        command.execute(&repo)?;

        assert_eq!(
            command.runner.calls,
            vec![
                RecordedCall {
                    program: "git".into(),
                    dir: worktree_path.clone(),
                    args: vec!["rev-parse".into(), "--abbrev-ref".into(), "HEAD".into()],
                },
                RecordedCall {
                    program: "gh".into(),
                    dir: repo_root.clone(),
                    args: vec![
                        "pr".into(),
                        "list".into(),
                        "--head".into(),
                        "feature/test".into(),
                        "--state".into(),
                        "open".into(),
                        "--json".into(),
                        "number".into(),
                        "--limit".into(),
                        "1".into(),
                    ],
                },
                RecordedCall {
                    program: "gh".into(),
                    dir: repo_root.clone(),
                    args: vec![
                        "pr".into(),
                        "merge".into(),
                        "42".into(),
                        "--merge".into(),
                        "--delete-branch".into(),
                    ],
                },
                RecordedCall {
                    program: "git".into(),
                    dir: worktree_path,
                    args: vec!["switch".into(), "feature/test".into()],
                },
            ]
        );

        Ok(())
    }

    #[test]
    fn skips_merge_when_no_pull_request_found() -> color_eyre::Result<()> {
        let repo_dir = TempDir::new()?;
        init_git_repo(&repo_dir)?;
        let repo = Repo::discover_from(repo_dir.path())?;
        let repo_root = repo.root().to_path_buf();
        let worktree_path = repo.worktrees_dir().join("feature/test");
        fs::create_dir_all(&worktree_path)?;

        let mut runner = MockCommandRunner::default();
        runner.responses.extend([
            Ok(CommandOutput {
                stdout: "feature/test\n".into(),
                stderr: String::new(),
                success: true,
                status_code: Some(0),
            }),
            Ok(CommandOutput {
                stdout: "[]".into(),
                stderr: String::new(),
                success: true,
                status_code: Some(0),
            }),
        ]);

        let mut command = MergePrGithubCommand::with_runner("feature/test".into(), runner);
        command.execute(&repo)?;

        assert_eq!(
            command.runner.calls,
            vec![
                RecordedCall {
                    program: "git".into(),
                    dir: worktree_path.clone(),
                    args: vec!["rev-parse".into(), "--abbrev-ref".into(), "HEAD".into()],
                },
                RecordedCall {
                    program: "gh".into(),
                    dir: repo_root,
                    args: vec![
                        "pr".into(),
                        "list".into(),
                        "--head".into(),
                        "feature/test".into(),
                        "--state".into(),
                        "open".into(),
                        "--json".into(),
                        "number".into(),
                        "--limit".into(),
                        "1".into(),
                    ],
                },
            ]
        );

        Ok(())
    }

    #[test]
    fn surfaces_command_failures() -> color_eyre::Result<()> {
        let repo_dir = TempDir::new()?;
        init_git_repo(&repo_dir)?;
        let repo = Repo::discover_from(repo_dir.path())?;
        let worktree_path = repo.worktrees_dir().join("feature/test");
        fs::create_dir_all(&worktree_path)?;

        let mut runner = MockCommandRunner::default();
        runner.responses.push_back(Ok(CommandOutput {
            stdout: String::from(""),
            stderr: String::from("fatal: bad revision"),
            success: false,
            status_code: Some(128),
        }));

        let mut command = MergePrGithubCommand::with_runner("feature/test".into(), runner);
        let err = command.execute(&repo).unwrap_err();
        assert!(err.to_string().contains("git rev-parse"));
        Ok(())
    }

    #[test]
    fn surfaces_switch_failures() -> color_eyre::Result<()> {
        let repo_dir = TempDir::new()?;
        init_git_repo(&repo_dir)?;
        let repo = Repo::discover_from(repo_dir.path())?;
        let repo_root = repo.root().to_path_buf();
        let worktree_path = repo.worktrees_dir().join("feature/test");
        fs::create_dir_all(&worktree_path)?;

        let mut runner = MockCommandRunner::default();
        runner.responses.extend([
            Ok(CommandOutput {
                stdout: "feature/test\n".into(),
                stderr: String::new(),
                success: true,
                status_code: Some(0),
            }),
            Ok(CommandOutput {
                stdout: "[{\"number\":42}]".into(),
                stderr: String::new(),
                success: true,
                status_code: Some(0),
            }),
            Ok(CommandOutput {
                stdout: String::new(),
                stderr: String::new(),
                success: true,
                status_code: Some(0),
            }),
            Ok(CommandOutput {
                stdout: String::new(),
                stderr: String::from("fatal: not a git repository"),
                success: false,
                status_code: Some(128),
            }),
        ]);

        let mut command = MergePrGithubCommand::with_runner("feature/test".into(), runner);
        let err = command.execute(&repo).unwrap_err();
        assert!(err.to_string().contains("git switch"));

        assert_eq!(
            command.runner.calls,
            vec![
                RecordedCall {
                    program: "git".into(),
                    dir: worktree_path.clone(),
                    args: vec!["rev-parse".into(), "--abbrev-ref".into(), "HEAD".into()],
                },
                RecordedCall {
                    program: "gh".into(),
                    dir: repo_root.clone(),
                    args: vec![
                        "pr".into(),
                        "list".into(),
                        "--head".into(),
                        "feature/test".into(),
                        "--state".into(),
                        "open".into(),
                        "--json".into(),
                        "number".into(),
                        "--limit".into(),
                        "1".into(),
                    ],
                },
                RecordedCall {
                    program: "gh".into(),
                    dir: repo_root,
                    args: vec![
                        "pr".into(),
                        "merge".into(),
                        "42".into(),
                        "--merge".into(),
                        "--delete-branch".into(),
                    ],
                },
                RecordedCall {
                    program: "git".into(),
                    dir: worktree_path,
                    args: vec!["switch".into(), "feature/test".into()],
                },
            ]
        );

        Ok(())
    }
}
