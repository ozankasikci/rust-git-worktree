use std::{io, process::Command};

use color_eyre::{Result, eyre::WrapErr};
use crossterm::{
    event::Event,
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use crate::{
    Repo,
    commands::{
        cd::{CdCommand, shell_command},
        create::{CreateCommand, CreateOutcome},
        list::{find_worktrees, format_worktree},
        merge_pr_github::MergePrGithubCommand,
        pr_github::{PrGithubCommand, PrGithubOptions},
        rm::RemoveCommand,
    },
    editor::launch_worktree,
};

use super::{EventSource, Selection, WorktreeEntry, command::InteractiveCommand};

pub struct CrosstermEvents;

impl Default for CrosstermEvents {
    fn default() -> Self {
        Self
    }
}

impl EventSource for CrosstermEvents {
    fn next(&mut self) -> Result<Event> {
        crossterm::event::read().wrap_err("failed to read terminal event")
    }
}

pub fn run(repo: &Repo) -> Result<()> {
    let worktrees_dir = repo.ensure_worktrees_dir()?;
    let raw_entries = find_worktrees(&worktrees_dir)?;
    let worktrees = raw_entries
        .into_iter()
        .map(|path| {
            let display = format_worktree(&path);
            WorktreeEntry::new(display, worktrees_dir.join(&path))
        })
        .collect::<Vec<_>>();

    let (branches, default_branch) = load_branches(repo)?;

    enable_raw_mode().wrap_err("failed to enable raw mode")?;
    execute!(io::stdout(), EnterAlternateScreen).wrap_err("failed to enter alternate screen")?;

    let backend = CrosstermBackend::new(io::stdout());
    let terminal = Terminal::new(backend).wrap_err("failed to initialize terminal")?;
    let events = CrosstermEvents::default();

    let command = InteractiveCommand::new(
        terminal,
        events,
        worktrees_dir.clone(),
        worktrees,
        branches,
        default_branch,
    );
    let result = command.run(
        |name, remove_local_branch| {
            let command = RemoveCommand::new(name.to_owned(), false)
                .with_quiet(true)
                .with_remove_local_branch(remove_local_branch)
                .with_spawn_shell(false);
            command.execute(repo)
        },
        |name, base| {
            let command = CreateCommand::new(name.to_owned(), base.map(|b| b.to_owned()));
            match command.create_without_enter(repo, true)? {
                CreateOutcome::Created => Ok(()),
                CreateOutcome::AlreadyExists => Err(color_eyre::eyre::eyre!(
                    "Worktree `{}` already exists.",
                    name
                )),
            }
        },
        |name, path| launch_worktree(repo, name, path, true),
    );
    let cleanup_result = cleanup_terminal();

    let selection = match (result, cleanup_result) {
        (Ok(selection), Ok(())) => selection,
        (Err(run_err), Ok(())) => return Err(run_err),
        (Ok(_), Err(cleanup_err)) => return Err(cleanup_err),
        (Err(run_err), Err(cleanup_err)) => {
            return Err(color_eyre::eyre::eyre!(
                "interactive session failed ({run_err}); cleanup failed: {cleanup_err}"
            ));
        }
    };

    if let Some(selection) = selection {
        match selection {
            Selection::RepoRoot => {
                cd_repo_root(repo)?;
            }
            Selection::Worktree(name) => {
                let command = CdCommand::new(name, false);
                command.execute(repo)?;
            }
            Selection::PrGithub(name) => {
                let options = PrGithubOptions {
                    name,
                    push: true,
                    draft: false,
                    fill: false,
                    web: false,
                    remote: String::from("origin"),
                    reviewers: Vec::new(),
                    extra_args: Vec::new(),
                };
                let mut command = PrGithubCommand::new(options);
                command.execute(repo)?;
            }
            Selection::MergePrGithub {
                name,
                remove_local_branch,
                remove_remote_branch,
                remove_worktree,
            } => {
                let mut command = MergePrGithubCommand::new(name.clone());
                if !remove_local_branch {
                    command.disable_remove_local();
                }
                if remove_remote_branch {
                    command.enable_remove_remote();
                }
                command.execute(repo)?;

                if remove_worktree {
                    let remove_command = RemoveCommand::new(name, false);
                    let _ = remove_command.execute(repo)?;
                }
            }
        }
    }

    Ok(())
}

fn cleanup_terminal() -> Result<()> {
    disable_raw_mode().wrap_err("failed to disable raw mode")?;
    execute!(io::stdout(), LeaveAlternateScreen).wrap_err("failed to leave alternate screen")?;
    Ok(())
}

fn cd_repo_root(repo: &Repo) -> Result<()> {
    let root = repo.root();
    if !root.exists() {
        return Err(color_eyre::eyre::eyre!(
            "repository root `{}` does not exist",
            root.display()
        ));
    }

    let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());

    let (program, args) = shell_command();

    let mut cmd = Command::new(&program);
    cmd.args(&args);
    cmd.current_dir(&canonical);
    cmd.env("PWD", canonical.as_os_str());

    cmd.status()
        .wrap_err("failed to spawn subshell")?
        .success()
        .then_some(())
        .ok_or_else(|| color_eyre::eyre::eyre!("subshell exited with a non-zero status"))
}

fn load_branches(repo: &Repo) -> Result<(Vec<String>, Option<String>)> {
    use std::collections::BTreeSet;

    use git2::BranchType;

    let git_repo = repo.git();
    let mut set = BTreeSet::new();
    let mut default_branch = None;

    if let Ok(head) = git_repo.head() {
        if head.is_branch() {
            if let Some(name) = head.shorthand() {
                let branch = name.to_string();
                set.insert(branch.clone());
                default_branch = Some(branch);
            }
        }
    }

    let mut iter = git_repo.branches(Some(BranchType::Local))?;
    while let Some(branch_result) = iter.next() {
        let (branch, _) = branch_result?;
        if let Some(name) = branch.name()? {
            if !name.is_empty() {
                set.insert(name.to_string());
            }
        }
    }

    let branches: Vec<String> = set.into_iter().collect();
    let default_branch = default_branch.and_then(|branch| {
        if branches.iter().any(|candidate| candidate == &branch) {
            Some(branch)
        } else {
            None
        }
    });

    Ok((branches, default_branch))
}
