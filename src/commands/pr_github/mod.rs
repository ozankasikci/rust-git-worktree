use std::{
    fmt,
    path::{Path, PathBuf},
    process::Command,
};

use color_eyre::eyre::{self, WrapErr};
use owo_colors::{OwoColorize, Stream};

use crate::Repo;

#[derive(Debug)]
pub struct PrGithubCommand<R = SystemCommandRunner> {
    name: String,
    push: bool,
    draft: bool,
    fill: bool,
    web: bool,
    remote: String,
    reviewers: Vec<String>,
    extra_args: Vec<String>,
    runner: R,
}

impl PrGithubCommand {
    pub fn new(
        name: String,
        push: bool,
        draft: bool,
        fill: bool,
        web: bool,
        remote: String,
        reviewers: Vec<String>,
        extra_args: Vec<String>,
    ) -> Self {
        Self::with_runner(
            name,
            push,
            draft,
            fill,
            web,
            remote,
            reviewers,
            extra_args,
            SystemCommandRunner,
        )
    }
}

impl<R> PrGithubCommand<R>
where
    R: CommandRunner,
{
    pub fn with_runner(
        name: String,
        push: bool,
        draft: bool,
        fill: bool,
        web: bool,
        remote: String,
        reviewers: Vec<String>,
        extra_args: Vec<String>,
        runner: R,
    ) -> Self {
        Self {
            name,
            push,
            draft,
            fill,
            web,
            remote,
            reviewers,
            extra_args,
            runner,
        }
    }

    pub fn execute(&mut self, repo: &Repo) -> color_eyre::Result<()> {
        let worktree_path = self.ensure_worktree_path(repo)?;
        let branch = self.determine_branch(&worktree_path)?;

        let branch_label = format_with_color(&branch, |text| format!("{}", text.magenta().bold()));
        let path_label = format_with_color(&worktree_path.display().to_string(), |text| {
            format!("{}", text.blue())
        });
        println!(
            "Preparing GitHub PR for `{}` from `{}`...",
            branch_label, path_label
        );

        self.ensure_pr_metadata_options()?;

        if self.push {
            self.push_branch(&worktree_path, &branch)?;
        } else {
            let message = format!("Skipping push for `{}` (push disabled).", branch_label);
            println!(
                "{}",
                message.if_supports_color(Stream::Stdout, |text| { format!("{}", text.dimmed()) })
            );
        }

        self.create_pull_request(&worktree_path, &branch)
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

    fn push_branch(&mut self, worktree_path: &Path, branch: &str) -> color_eyre::Result<()> {
        let args = vec![
            "push".to_owned(),
            "-u".to_owned(),
            self.remote.clone(),
            branch.to_owned(),
        ];
        let output = self
            .runner
            .run("git", worktree_path, &args)
            .wrap_err("failed to run `git push`")?;

        if !output.success {
            return Err(command_failure("git", &args, &output));
        }

        let remote_label = format_with_color(&self.remote, |text| format!("{}", text.green()));
        let branch_label = format_with_color(branch, |text| format!("{}", text.magenta().bold()));
        println!("Pushed `{}` to remote `{}`.", branch_label, remote_label);

        Ok(())
    }

    fn create_pull_request(
        &mut self,
        worktree_path: &Path,
        branch: &str,
    ) -> color_eyre::Result<()> {
        let mut args = vec!["pr".to_owned(), "create".to_owned()];
        args.push("--head".to_owned());
        args.push(branch.to_owned());

        if self.draft {
            args.push("--draft".to_owned());
        }
        if self.fill {
            args.push("--fill".to_owned());
        }
        if self.web {
            args.push("--web".to_owned());
        }

        for reviewer in &self.reviewers {
            args.push("--reviewer".to_owned());
            args.push(reviewer.clone());
        }

        args.extend(self.extra_args.clone());

        let output = self
            .runner
            .run("gh", worktree_path, &args)
            .wrap_err("failed to run `gh pr create`")?;

        if !output.success {
            return Err(command_failure("gh", &args, &output));
        }

        let branch_label = format_with_color(branch, |text| format!("{}", text.magenta().bold()));
        println!("GitHub pull request created for `{}`.", branch_label);
        Ok(())
    }

    fn ensure_pr_metadata_options(&self) -> color_eyre::Result<()> {
        if self.fill || self.web {
            return Ok(());
        }

        if self
            .extra_args
            .iter()
            .any(|arg| metadata_flag_allows_noninteractive(arg))
        {
            return Ok(());
        }

        Err(eyre::eyre!(
            "`rsworktree pr-github` runs `gh pr create` in non-interactive mode. Provide PR metadata with `--fill`, `--title/--body`, or use `--web` to open the browser."
        ))
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

fn format_with_color(value: &str, paint: impl Fn(&str) -> String) -> String {
    value
        .if_supports_color(Stream::Stdout, |text| paint(text))
        .to_string()
}

fn format_command(program: &str, args: &[String]) -> String {
    let mut parts = Vec::with_capacity(1 + args.len());
    parts.push(quote_arg(program));
    for arg in args {
        parts.push(quote_arg(arg));
    }
    parts.join(" ")
}

fn metadata_flag_allows_noninteractive(arg: &str) -> bool {
    let cleaned = arg.trim();
    if cleaned == "--" {
        return false;
    }

    matches!(
        cleaned,
        "--fill"
            | "-f"
            | "--fill-first"
            | "--fill-verbose"
            | "--web"
            | "-w"
            | "--title"
            | "-t"
            | "--body"
            | "-b"
            | "--body-file"
            | "-F"
    ) || cleaned.starts_with("--title=")
        || cleaned.starts_with("--body=")
        || cleaned.starts_with("--body-file=")
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

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
    pub status_code: Option<i32>,
}

pub trait CommandRunner {
    fn run(
        &mut self,
        program: &str,
        current_dir: &Path,
        args: &[String],
    ) -> color_eyre::Result<CommandOutput>;
}

#[derive(Debug, Clone, Default)]
pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn run(
        &mut self,
        program: &str,
        current_dir: &Path,
        args: &[String],
    ) -> color_eyre::Result<CommandOutput> {
        let output = Command::new(program)
            .current_dir(current_dir)
            .args(args)
            .output()
            .wrap_err_with(|| {
                eyre::eyre!("failed to execute `{}`", format_command(program, args))
            })?;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            success: output.status.success(),
            status_code: output.status.code(),
        })
    }
}

impl fmt::Display for CommandOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "success: {}, status_code: {:?}, stdout: {:?}, stderr: {:?}",
            self.success, self.status_code, self.stdout, self.stderr
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::VecDeque, fs};

    use tempfile::TempDir;

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
        let status = Command::new(program)
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
    fn executes_push_and_gh() -> color_eyre::Result<()> {
        let repo_dir = TempDir::new()?;
        init_git_repo(&repo_dir)?;
        let repo = Repo::discover_from(repo_dir.path())?;
        let worktree_path = repo.worktrees_dir().join("feature/test");
        fs::create_dir_all(&worktree_path)?;

        let mut runner = MockCommandRunner::default();
        runner.responses.push_back(Ok(CommandOutput {
            stdout: "feature/test\n".into(),
            stderr: String::new(),
            success: true,
            status_code: Some(0),
        }));
        runner.responses.push_back(Ok(CommandOutput {
            stdout: String::new(),
            stderr: String::new(),
            success: true,
            status_code: Some(0),
        }));
        runner.responses.push_back(Ok(CommandOutput {
            stdout: String::new(),
            stderr: String::new(),
            success: true,
            status_code: Some(0),
        }));

        let mut command = PrGithubCommand::with_runner(
            "feature/test".into(),
            true,
            false,
            true,
            false,
            "origin".into(),
            vec!["octocat".into()],
            vec!["--label".into(), "ready".into()],
            runner,
        );

        command.execute(&repo)?;

        let expected_calls = vec![
            RecordedCall {
                program: "git".into(),
                dir: worktree_path.clone(),
                args: vec!["rev-parse".into(), "--abbrev-ref".into(), "HEAD".into()],
            },
            RecordedCall {
                program: "git".into(),
                dir: worktree_path.clone(),
                args: vec![
                    "push".into(),
                    "-u".into(),
                    "origin".into(),
                    "feature/test".into(),
                ],
            },
            RecordedCall {
                program: "gh".into(),
                dir: worktree_path.clone(),
                args: vec![
                    "pr".into(),
                    "create".into(),
                    "--head".into(),
                    "feature/test".into(),
                    "--fill".into(),
                    "--reviewer".into(),
                    "octocat".into(),
                    "--label".into(),
                    "ready".into(),
                ],
            },
        ];

        assert_eq!(command.runner.calls, expected_calls);

        Ok(())
    }

    #[test]
    fn skips_push_when_disabled() -> color_eyre::Result<()> {
        let repo_dir = TempDir::new()?;
        init_git_repo(&repo_dir)?;
        let repo = Repo::discover_from(repo_dir.path())?;
        let worktree_path = repo.worktrees_dir().join("feature/test");
        fs::create_dir_all(&worktree_path)?;

        let mut runner = MockCommandRunner::default();
        runner.responses.push_back(Ok(CommandOutput {
            stdout: "feature/test\n".into(),
            stderr: String::new(),
            success: true,
            status_code: Some(0),
        }));
        runner.responses.push_back(Ok(CommandOutput {
            stdout: String::new(),
            stderr: String::new(),
            success: true,
            status_code: Some(0),
        }));

        let mut command = PrGithubCommand::with_runner(
            "feature/test".into(),
            false,
            true,
            true,
            true,
            "origin".into(),
            Vec::new(),
            Vec::new(),
            runner,
        );

        command.execute(&repo)?;

        let expected_calls = vec![
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
                    "create".into(),
                    "--head".into(),
                    "feature/test".into(),
                    "--draft".into(),
                    "--fill".into(),
                    "--web".into(),
                ],
            },
        ];

        assert_eq!(command.runner.calls, expected_calls);

        Ok(())
    }

    #[test]
    fn errors_when_worktree_missing() -> color_eyre::Result<()> {
        let repo_dir = TempDir::new()?;
        init_git_repo(&repo_dir)?;
        let repo = Repo::discover_from(repo_dir.path())?;

        let mut command = PrGithubCommand::with_runner(
            "missing".into(),
            true,
            false,
            false,
            false,
            "origin".into(),
            Vec::new(),
            Vec::new(),
            MockCommandRunner::default(),
        );

        let err = command.execute(&repo).unwrap_err();
        assert!(err.to_string().contains("does not exist"));
        Ok(())
    }

    #[test]
    fn surfaces_command_failure() -> color_eyre::Result<()> {
        let repo_dir = TempDir::new()?;
        init_git_repo(&repo_dir)?;
        let repo = Repo::discover_from(repo_dir.path())?;
        let worktree_path = repo.worktrees_dir().join("feature/test");
        fs::create_dir_all(&worktree_path)?;

        let mut runner = MockCommandRunner::default();
        runner.responses.push_back(Ok(CommandOutput {
            stdout: String::new(),
            stderr: "fatal: detached HEAD".into(),
            success: false,
            status_code: Some(128),
        }));

        let mut command = PrGithubCommand::with_runner(
            "feature/test".into(),
            true,
            false,
            false,
            false,
            "origin".into(),
            Vec::new(),
            Vec::new(),
            runner,
        );

        let err = command.execute(&repo).unwrap_err();
        assert!(err.to_string().contains("git rev-parse"));
        Ok(())
    }

    #[test]
    fn errors_when_missing_metadata_flags() -> color_eyre::Result<()> {
        let repo_dir = TempDir::new()?;
        init_git_repo(&repo_dir)?;
        let repo = Repo::discover_from(repo_dir.path())?;
        let worktree_path = repo.worktrees_dir().join("feature/test");
        fs::create_dir_all(&worktree_path)?;

        let mut runner = MockCommandRunner::default();
        runner.responses.push_back(Ok(CommandOutput {
            stdout: "feature/test\n".into(),
            stderr: String::new(),
            success: true,
            status_code: Some(0),
        }));

        let mut command = PrGithubCommand::with_runner(
            "feature/test".into(),
            true,
            false,
            false,
            false,
            "origin".into(),
            Vec::new(),
            Vec::new(),
            runner,
        );

        let err = command.execute(&repo).unwrap_err();
        assert!(
            err.to_string()
                .contains("Provide PR metadata with `--fill`")
        );

        assert_eq!(
            command.runner.calls,
            vec![RecordedCall {
                program: "git".into(),
                dir: worktree_path,
                args: vec!["rev-parse".into(), "--abbrev-ref".into(), "HEAD".into()],
            }]
        );

        Ok(())
    }
}
