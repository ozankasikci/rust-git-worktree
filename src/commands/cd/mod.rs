use std::process::Command;

use color_eyre::eyre::{self, WrapErr};
use owo_colors::{OwoColorize, Stream};

pub(crate) const SHELL_OVERRIDE_ENV: &str = "RSWORKTREE_SHELL";

use crate::Repo;

#[derive(Debug)]
pub struct CdCommand {
    name: String,
    print_only: bool,
}

impl CdCommand {
    pub fn new(name: String, print_only: bool) -> Self {
        Self { name, print_only }
    }

    pub fn execute(&self, repo: &Repo) -> color_eyre::Result<()> {
        let worktrees_dir = repo.ensure_worktrees_dir()?;
        let worktree_path = worktrees_dir.join(&self.name);

        if !worktree_path.exists() {
            return Err(eyre::eyre!(
                "worktree `{}` does not exist under `{}`",
                self.name,
                worktrees_dir.display()
            ));
        }

        let canonical = worktree_path
            .canonicalize()
            .wrap_err_with(|| eyre::eyre!("failed to resolve `{}`", worktree_path.display()))?;

        if self.print_only {
            let path_raw = format!("{}", canonical.display());
            let path = format!(
                "{}",
                path_raw
                    .as_str()
                    .if_supports_color(Stream::Stdout, |text| { format!("{}", text.blue()) })
            );
            println!("{}", path);
            return Ok(());
        }

        let (program, args) = shell_command();

        let mut cmd = Command::new(&program);
        cmd.args(args);
        cmd.current_dir(&canonical);
        cmd.env("PWD", canonical.as_os_str());
        cmd.status()
            .wrap_err("failed to spawn subshell")?
            .success()
            .then_some(())
            .ok_or_else(|| eyre::eyre!("subshell exited with a non-zero status"))
    }
}

pub(crate) fn shell_command() -> (String, Vec<String>) {
    if let Ok(override_shell) = std::env::var(SHELL_OVERRIDE_ENV) {
        if !override_shell.trim().is_empty() {
            return (override_shell, Vec::new());
        }
    }

    if let Ok(shell) = std::env::var("SHELL") {
        if !shell.trim().is_empty() {
            return (shell, vec!["-i".into()]);
        }
    }

    ("/bin/sh".into(), vec!["-i".into()])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        env, fs,
        process::Command as StdCommand,
        sync::{Mutex, OnceLock},
    };

    use tempfile::TempDir;

    use crate::{Repo, commands::create::CreateCommand};

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
    fn prints_canonical_path_when_worktree_exists() -> color_eyre::Result<()> {
        let dir = TempDir::new()?;
        init_git_repo(&dir)?;
        let repo = Repo::discover_from(dir.path())?;

        let create = CreateCommand::new("feature/test".into(), None);
        unsafe {
            std::env::set_var(SHELL_OVERRIDE_ENV, "env");
        }
        create.execute(&repo)?;

        let command = CdCommand::new("feature/test".into(), true);
        command.execute(&repo)?;

        Ok(())
    }

    #[test]
    fn errors_when_missing_worktree() {
        let dir = TempDir::new().unwrap();
        init_git_repo(&dir).unwrap();
        let repo = Repo::discover_from(dir.path()).unwrap();
        let command = CdCommand::new("missing".into(), true);
        assert!(command.execute(&repo).is_err());
    }

    struct EnvGuard {
        key: &'static str,
        previous: Option<std::ffi::OsString>,
        had_value: bool,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = env::var_os(key);
            unsafe {
                env::set_var(key, value);
            }
            Self {
                key,
                previous,
                had_value: true,
            }
        }

        fn remove(key: &'static str) -> Self {
            let previous = env::var_os(key);
            unsafe {
                env::remove_var(key);
            }
            Self {
                key,
                previous,
                had_value: false,
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(value) = self.previous.take() {
                unsafe {
                    env::set_var(self.key, value);
                }
            } else if self.had_value {
                unsafe {
                    env::remove_var(self.key);
                }
            }
        }
    }

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn shell_command_prefers_override_env() {
        let _lock = env_lock().lock().unwrap();
        let _shell_guard = EnvGuard::remove("SHELL");
        let _override_guard = EnvGuard::set(SHELL_OVERRIDE_ENV, "/custom/shell");

        let (program, args) = shell_command();
        assert_eq!(program, "/custom/shell");
        assert!(args.is_empty());
    }

    #[test]
    fn shell_command_uses_shell_env_when_override_missing() {
        let _lock = env_lock().lock().unwrap();
        let _override_guard = EnvGuard::remove(SHELL_OVERRIDE_ENV);
        let _shell_guard = EnvGuard::set("SHELL", "/bin/bash");

        let (program, args) = shell_command();
        assert_eq!(program, "/bin/bash");
        assert_eq!(args, vec![String::from("-i")]);
    }

    #[test]
    fn shell_command_falls_back_to_default() {
        let _lock = env_lock().lock().unwrap();
        let _override_guard = EnvGuard::set(SHELL_OVERRIDE_ENV, "");
        let _shell_guard = EnvGuard::set("SHELL", "");

        let (program, args) = shell_command();
        assert_eq!(program, "/bin/sh");
        assert_eq!(args, vec![String::from("-i")]);
    }
}
