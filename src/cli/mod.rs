use std::env;

use clap::{Parser, Subcommand};

use color_eyre::eyre::{self, WrapErr};

use crate::{
    Repo,
    commands::{
        cd::CdCommand, create::CreateCommand, list::ListCommand, pr_github::PrGithubCommand,
        rm::RemoveCommand,
    },
};

#[derive(Parser, Debug)]
#[command(name = "rsworktree", version, about = "Manage Git worktrees more easily", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Create a worktree under the repo-local `.rsworktree` directory.
    Create(CreateArgs),
    /// List worktrees managed in `.rsworktree`.
    Ls,
    /// Open a shell in the given worktree.
    Cd(CdArgs),
    /// Remove a worktree tracked in `.rsworktree`.
    Rm(RmArgs),
    /// Create a GitHub pull request for the worktree's branch using the GitHub CLI.
    PrGithub(PrGithubArgs),
}

#[derive(Parser, Debug)]
struct CreateArgs {
    /// Name of the worktree (also used as the branch name)
    name: String,
    /// Branch to base the new worktree branch on
    #[arg(long)]
    base: Option<String>,
}

#[derive(Parser, Debug)]
struct CdArgs {
    /// Name of the worktree to enter
    name: String,
    /// Only print the resolved worktree path
    #[arg(long)]
    print: bool,
}

#[derive(Parser, Debug)]
struct RmArgs {
    /// Name of the worktree to remove
    name: String,
    /// Force removal even if the worktree has uncommitted changes
    #[arg(long)]
    force: bool,
}

#[derive(Parser, Debug)]
struct PrGithubArgs {
    /// Name of the worktree to prepare a PR from (defaults to the current worktree)
    name: Option<String>,
    /// Skip pushing the branch before creating the PR
    #[arg(long = "no-push")]
    no_push: bool,
    /// Mark the PR as a draft
    #[arg(long)]
    draft: bool,
    /// Prefill the PR title and body from commits
    #[arg(long)]
    fill: bool,
    /// Open the PR creation flow in the browser
    #[arg(long)]
    web: bool,
    /// Remote to push the branch to before creating the PR
    #[arg(long, default_value = "origin")]
    remote: String,
    /// Request reviews from the given GitHub handles
    #[arg(long = "reviewer", value_name = "login")]
    reviewers: Vec<String>,
    /// Additional arguments passed directly to `gh pr create`
    #[arg(last = true, value_name = "ARG")]
    extra: Vec<String>,
}

pub fn run() -> color_eyre::Result<()> {
    let cli = Cli::parse();
    let repo = Repo::discover()?;

    match cli.command {
        Commands::Create(args) => {
            let command = CreateCommand::new(args.name, args.base);
            command.execute(&repo)?;
        }
        Commands::Ls => {
            let command = ListCommand::default();
            command.execute(&repo)?;
        }
        Commands::Cd(args) => {
            let command = CdCommand::new(args.name, args.print);
            command.execute(&repo)?;
        }
        Commands::Rm(args) => {
            let command = RemoveCommand::new(args.name, args.force);
            command.execute(&repo)?;
        }
        Commands::PrGithub(args) => {
            let worktree_name = resolve_worktree_name(args.name, &repo)?;
            let mut command = PrGithubCommand::new(
                worktree_name,
                !args.no_push,
                args.draft,
                args.fill,
                args.web,
                args.remote,
                args.reviewers,
                args.extra,
            );
            command.execute(&repo)?;
        }
    }

    Ok(())
}

fn resolve_worktree_name(name: Option<String>, repo: &Repo) -> color_eyre::Result<String> {
    if let Some(name) = name {
        return Ok(name);
    }

    let cwd = env::current_dir().wrap_err("failed to read current directory")?;
    let canonical_cwd = cwd.canonicalize().unwrap_or(cwd);

    let worktrees_dir = repo.ensure_worktrees_dir()?;
    let canonical_worktrees_dir = worktrees_dir
        .canonicalize()
        .unwrap_or_else(|_| worktrees_dir.clone());

    if !canonical_cwd.starts_with(&canonical_worktrees_dir) {
        return Err(eyre::eyre!(
            "`rsworktree pr-github` without <name> must be run from inside `{}`. Current directory: `{}`.",
            worktrees_dir.display(),
            canonical_cwd.display()
        ));
    }

    let relative = canonical_cwd
        .strip_prefix(&canonical_worktrees_dir)
        .wrap_err("failed to compute path relative to worktrees directory")?;

    let components = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    if components.is_empty() {
        return Err(eyre::eyre!(
            "Run `rsworktree pr-github` from inside a specific worktree (e.g. `.rsworktree/<name>`)."
        ));
    }

    Ok(components.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs, path::Path, process::Command as StdCommand};

    use color_eyre::eyre::{self, WrapErr};

    use tempfile::TempDir;

    struct DirGuard {
        original: std::path::PathBuf,
    }

    impl DirGuard {
        fn change_to(path: &Path) -> color_eyre::Result<Self> {
            let original = env::current_dir().wrap_err("failed to capture current directory")?;
            env::set_current_dir(path)
                .wrap_err_with(|| eyre::eyre!("failed to switch to `{}`", path.display()))?;
            Ok(Self { original })
        }
    }

    impl Drop for DirGuard {
        fn drop(&mut self) {
            let _ = env::set_current_dir(&self.original);
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
        let status = StdCommand::new(program)
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
    fn resolve_worktree_name_returns_cli_argument_when_present() -> color_eyre::Result<()> {
        let repo_dir = TempDir::new()?;
        init_git_repo(&repo_dir)?;
        let repo = Repo::discover_from(repo_dir.path())?;

        let resolved = resolve_worktree_name(Some("feature/test".into()), &repo)?;
        assert_eq!(resolved, "feature/test");

        Ok(())
    }

    #[test]
    fn resolve_worktree_name_infers_from_cwd_inside_worktree() -> color_eyre::Result<()> {
        let repo_dir = TempDir::new()?;
        init_git_repo(&repo_dir)?;
        let repo = Repo::discover_from(repo_dir.path())?;
        let worktree_dir = repo.ensure_worktrees_dir()?.join("feature/nested");
        fs::create_dir_all(&worktree_dir)?;

        let _guard = DirGuard::change_to(&worktree_dir)?;
        let resolved = resolve_worktree_name(None, &repo)?;
        assert_eq!(resolved, "feature/nested");

        Ok(())
    }

    #[test]
    fn resolve_worktree_name_requires_running_inside_worktree() -> color_eyre::Result<()> {
        let repo_dir = TempDir::new()?;
        init_git_repo(&repo_dir)?;
        let repo = Repo::discover_from(repo_dir.path())?;
        let _guard = DirGuard::change_to(repo.root())?;

        let err = resolve_worktree_name(None, &repo).unwrap_err();
        assert!(err.to_string().contains("must be run from inside"));

        Ok(())
    }

    #[test]
    fn resolve_worktree_name_rejects_rsworktree_root() -> color_eyre::Result<()> {
        let repo_dir = TempDir::new()?;
        init_git_repo(&repo_dir)?;
        let repo = Repo::discover_from(repo_dir.path())?;
        let worktrees_dir = repo.ensure_worktrees_dir()?;
        let _guard = DirGuard::change_to(&worktrees_dir)?;

        let err = resolve_worktree_name(None, &repo).unwrap_err();
        assert!(
            err.to_string()
                .contains("Run `rsworktree pr-github` from inside")
        );

        Ok(())
    }
}
