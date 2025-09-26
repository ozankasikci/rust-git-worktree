use std::path::{Path, PathBuf};

use color_eyre::eyre::{self, WrapErr};
use owo_colors::{OwoColorize, Stream};
use serde::Deserialize;

use crate::{
    Repo,
    commands::pr_github::{CommandOutput, CommandRunner, SystemCommandRunner},
};

#[derive(Debug)]
pub struct MergeCommand<R = SystemCommandRunner> {
    name: String,
    runner: R,
}

impl MergeCommand {
    pub fn new(name: String) -> Self {
        Self::with_runner(name, SystemCommandRunner)
    }
}

impl<R> MergeCommand<R>
where
    R: CommandRunner,
{
    pub fn with_runner(name: String, runner: R) -> Self {
        Self { name, runner }
    }

    pub fn execute(&mut self, repo: &Repo) -> color_eyre::Result<()> {
        let worktree_path = self.ensure_worktree_path(repo)?;
        let branch = self.determine_branch(&worktree_path)?;

        let branch_label = format_with_color(&branch, |text| format!("{}", text.magenta().bold()));
        let path_label = format_with_color(&worktree_path.display().to_string(), |text| {
            format!("{}", text.blue())
        });
        println!(
            "Looking for open PR for `{}` from `{}`...",
            branch_label, path_label
        );

        match self.find_pull_request(&worktree_path, &branch)? {
            Some(pr_number) => self.merge_pull_request(&worktree_path, pr_number, &branch),
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
        worktree_path: &Path,
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
            .run("gh", worktree_path, &args)
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
        worktree_path: &Path,
        pr_number: u64,
        branch: &str,
    ) -> color_eyre::Result<()> {
        let args = vec![
            "pr".to_owned(),
            "merge".to_owned(),
            pr_number.to_string(),
            "--merge".to_owned(),
            "--delete-branch".to_owned(),
        ];

        let output = self
            .runner
            .run("gh", worktree_path, &args)
            .wrap_err("failed to run `gh pr merge`")?;

        if !output.success {
            return Err(command_failure("gh", &args, &output));
        }

        let pr_label = format_with_color(&format!("#{}", pr_number), |text| {
            format!("{}", text.green().bold())
        });
        let branch_label = format_with_color(branch, |text| format!("{}", text.magenta().bold()));
        println!("Merged PR {} for branch `{}`.", pr_label, branch_label);
        Ok(())
    }
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
        ]);

        let mut command = MergeCommand::with_runner("feature/test".into(), runner);
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
                    dir: worktree_path.clone(),
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
                    dir: worktree_path,
                    args: vec![
                        "pr".into(),
                        "merge".into(),
                        "42".into(),
                        "--merge".into(),
                        "--delete-branch".into(),
                    ],
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

        let mut command = MergeCommand::with_runner("feature/test".into(), runner);
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
                    dir: worktree_path,
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

        let mut command = MergeCommand::with_runner("feature/test".into(), runner);
        let err = command.execute(&repo).unwrap_err();
        assert!(err.to_string().contains("git rev-parse"));
        Ok(())
    }
}
