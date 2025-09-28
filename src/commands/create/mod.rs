use std::fs;

use color_eyre::eyre::{self, Context};

use owo_colors::{OwoColorize, Stream};

use git2::{ErrorCode, WorktreeAddOptions};

use crate::{Repo, commands::cd::CdCommand};

#[derive(Debug)]
pub struct CreateCommand {
    name: String,
    base: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateOutcome {
    AlreadyExists,
    Created,
}

impl CreateCommand {
    pub fn new(name: String, base: Option<String>) -> Self {
        Self { name, base }
    }

    pub fn execute(&self, repo: &Repo) -> color_eyre::Result<()> {
        let outcome = self.create_internal(repo, false)?;
        match outcome {
            CreateOutcome::Created | CreateOutcome::AlreadyExists => self.enter_worktree(repo),
        }
    }

    pub fn create_without_enter(
        &self,
        repo: &Repo,
        quiet: bool,
    ) -> color_eyre::Result<CreateOutcome> {
        self.create_internal(repo, quiet)
    }

    fn enter_worktree(&self, repo: &Repo) -> color_eyre::Result<()> {
        CdCommand::new(self.name.clone(), false).execute(repo)
    }

    fn create_internal(&self, repo: &Repo, quiet: bool) -> color_eyre::Result<CreateOutcome> {
        let worktrees_dir = repo.ensure_worktrees_dir()?;
        let worktree_path = worktrees_dir.join(&self.name);
        let target_branch = self.name.as_str();
        let base_branch = self.base.as_deref();

        if worktree_path.exists() {
            if !quiet {
                let name = format!(
                    "{}",
                    self.name
                        .as_str()
                        .if_supports_color(Stream::Stdout, |text| {
                            format!("{}", text.cyan().bold())
                        })
                );
                println!(
                    "Worktree `{}` already exists at `{}`.",
                    name,
                    worktree_path.display()
                );
            }
            return Ok(CreateOutcome::AlreadyExists);
        }

        if let Some(parent) = worktree_path.parent() {
            fs::create_dir_all(parent).wrap_err_with(|| {
                eyre::eyre!("failed to prepare directory `{}`", parent.display())
            })?;
        }

        let git_repo = repo.git();
        let reference = prepare_branch(git_repo, target_branch, base_branch)?;
        let metadata_name = worktree_metadata_name(&self.name);
        let mut opts = WorktreeAddOptions::new();
        opts.reference(Some(&reference));
        git_repo
            .worktree(&metadata_name, &worktree_path, Some(&opts))
            .wrap_err_with(|| {
                eyre::eyre!(
                    "failed to add worktree `{}` at `{}`",
                    target_branch,
                    worktree_path.display()
                )
            })?;

        if !quiet {
            let name = format!(
                "{}",
                target_branch.if_supports_color(Stream::Stdout, |text| {
                    format!("{}", text.green().bold())
                })
            );
            let path_raw = format!("{}", worktree_path.display());
            let path = format!(
                "{}",
                path_raw
                    .as_str()
                    .if_supports_color(Stream::Stdout, |text| { format!("{}", text.blue()) })
            );
            if let Some(base) = base_branch {
                let base = format!(
                    "{}",
                    base.if_supports_color(Stream::Stdout, |text| {
                        format!("{}", text.magenta().bold())
                    })
                );
                println!("Created worktree `{}` at `{}` from `{}`.", name, path, base);
            } else {
                println!("Created worktree `{}` at `{}`.", name, path);
            }
        }

        Ok(CreateOutcome::Created)
    }
}

fn prepare_branch<'repo>(
    repo: &'repo git2::Repository,
    branch: &str,
    base: Option<&str>,
) -> color_eyre::Result<git2::Reference<'repo>> {
    let full_ref = format!("refs/heads/{branch}");
    match repo.find_reference(&full_ref) {
        Ok(reference) => Ok(reference),
        Err(err) if err.code() == ErrorCode::NotFound => {
            let base_name = base.unwrap_or("HEAD");
            let object = repo
                .revparse_single(base_name)
                .wrap_err_with(|| eyre::eyre!("failed to resolve base reference `{base_name}`"))?;
            let commit = object.peel_to_commit().wrap_err_with(|| {
                eyre::eyre!("base reference `{base_name}` does not point to a commit")
            })?;
            let branch = repo.branch(branch, &commit, false).wrap_err_with(|| {
                eyre::eyre!("failed to create branch `{branch}` from `{base_name}`")
            })?;
            Ok(branch.into_reference())
        }
        Err(err) => Err(eyre::eyre!("failed to look up branch `{branch}`: {err}")),
    }
}

fn worktree_metadata_name(name: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;

    let sanitized: String = name
        .chars()
        .map(|ch| match ch {
            '/' | '\\' => '-',
            ch if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') => ch,
            _ => '-',
        })
        .collect();

    let mut hasher = DefaultHasher::new();
    hasher.write(name.as_bytes());
    let hash = hasher.finish();

    let base = sanitized.trim_matches('-');
    let trimmed: String = if base.is_empty() {
        "worktree".into()
    } else {
        sanitized.chars().take(48).collect()
    };

    format!("rsworktree-{trimmed}-{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, process::Command as StdCommand};

    use tempfile::TempDir;

    use crate::{Repo, commands::cd::SHELL_OVERRIDE_ENV};

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
    fn creates_new_worktree_under_rsworktree_directory() -> color_eyre::Result<()> {
        let dir = TempDir::new()?;
        init_git_repo(&dir)?;

        let repo = Repo::discover_from(dir.path())?;
        unsafe {
            std::env::set_var(SHELL_OVERRIDE_ENV, "env");
        }
        let command = CreateCommand::new("feature/test".into(), None);
        command.execute(&repo)?;

        let expected_dir = repo.worktrees_dir().join("feature/test");
        assert!(
            expected_dir.exists(),
            "worktree directory should be created"
        );

        command.execute(&repo)?;

        let gitignore_path = repo.root().join(".gitignore");
        let gitignore_contents = fs::read_to_string(&gitignore_path)?;
        let occurrences = gitignore_contents
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                trimmed == ".rsworktree/" || trimmed == ".rsworktree"
            })
            .count();
        assert_eq!(
            occurrences, 1,
            "`.rsworktree/` entry should be present once"
        );

        Ok(())
    }

    fn split_metadata_name(name: &str) -> (&str, &str) {
        let without_prefix = name
            .strip_prefix("rsworktree-")
            .expect("name should include rsworktree prefix");
        without_prefix
            .rsplit_once('-')
            .expect("name should include trailing hash")
    }

    #[test]
    fn metadata_name_replaces_disallowed_characters() {
        let name = worktree_metadata_name("feat/branch with spaces");
        let (sanitized, hash) = split_metadata_name(&name);
        assert_eq!(sanitized, "feat-branch-with-spaces");
        assert_eq!(hash.len(), 16);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn metadata_name_uses_default_when_sanitized_empty() {
        let name = worktree_metadata_name("///");
        let (sanitized, _) = split_metadata_name(&name);
        assert_eq!(sanitized, "worktree");
    }

    #[test]
    fn metadata_name_truncates_long_inputs() {
        let long_name = "a".repeat(80);
        let name = worktree_metadata_name(&long_name);
        let (sanitized, _) = split_metadata_name(&name);
        assert_eq!(sanitized.len(), 48);
        assert!(sanitized.chars().all(|c| c == 'a'));
    }
}
