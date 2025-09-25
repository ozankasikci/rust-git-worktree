use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

use color_eyre::eyre::{self, Context};

const WORKTREE_IGNORE_ENTRY: &str = ".rsworktree/";
const WORKTREE_IGNORE_ALT_ENTRY: &str = ".rsworktree";

#[derive(Debug, Clone)]
pub struct Repo {
    root: PathBuf,
}

impl Repo {
    pub fn discover() -> color_eyre::Result<Self> {
        let cwd = std::env::current_dir().wrap_err("failed to read current directory")?;
        Self::discover_from(&cwd)
    }

    pub fn discover_from<P: AsRef<Path>>(path: P) -> color_eyre::Result<Self> {
        let path = path.as_ref();

        let output = Command::new("git")
            .current_dir(path)
            .args(["rev-parse", "--git-common-dir"])
            .output()
            .wrap_err("failed to run `git rev-parse --git-common-dir`")?;

        if !output.status.success() {
            return Err(eyre::eyre!(
                "not inside a git repository: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let git_dir_raw = String::from_utf8(output.stdout)
            .wrap_err("invalid UTF-8 in git common dir path")?
            .trim()
            .to_owned();

        let git_dir_path = PathBuf::from(&git_dir_raw);
        let git_dir_abs = if git_dir_path.is_absolute() {
            git_dir_path
        } else {
            fs::canonicalize(path.join(&git_dir_path)).unwrap_or_else(|_| path.join(git_dir_raw))
        };

        let root = git_dir_abs
            .parent()
            .ok_or_else(|| {
                eyre::eyre!(
                    "failed to determine repository root from `{}`",
                    git_dir_abs.display()
                )
            })?
            .to_path_buf();

        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn worktrees_dir(&self) -> PathBuf {
        self.root.join(".rsworktree")
    }

    pub fn ensure_worktrees_dir(&self) -> color_eyre::Result<PathBuf> {
        self.ensure_gitignore_entry()?;
        let dir = self.worktrees_dir();
        fs::create_dir_all(&dir)
            .wrap_err_with(|| eyre::eyre!("failed to create `{}`", dir.display()))?;
        Ok(dir)
    }

    fn ensure_gitignore_entry(&self) -> color_eyre::Result<()> {
        let gitignore_path = self.root.join(".gitignore");

        if gitignore_path.exists() {
            let contents = fs::read_to_string(&gitignore_path)
                .wrap_err_with(|| eyre::eyre!("failed to read `{}`", gitignore_path.display()))?;

            if gitignore_has_entry(&contents) {
                return Ok(());
            }

            let mut file = OpenOptions::new()
                .append(true)
                .open(&gitignore_path)
                .wrap_err_with(|| eyre::eyre!("failed to open `{}`", gitignore_path.display()))?;

            if !contents.is_empty() && !contents.ends_with('\n') {
                file.write_all(b"\n").wrap_err_with(|| {
                    eyre::eyre!("failed to update `{}`", gitignore_path.display())
                })?;
            }

            file.write_all(WORKTREE_IGNORE_ENTRY.as_bytes())
                .wrap_err_with(|| {
                    eyre::eyre!("failed to append to `{}`", gitignore_path.display())
                })?;
            file.write_all(b"\n").wrap_err_with(|| {
                eyre::eyre!("failed to append newline to `{}`", gitignore_path.display())
            })?;
        } else {
            fs::write(&gitignore_path, format!("{WORKTREE_IGNORE_ENTRY}\n"))
                .wrap_err_with(|| eyre::eyre!("failed to write `{}`", gitignore_path.display()))?;
        }

        Ok(())
    }
}

fn gitignore_has_entry(contents: &str) -> bool {
    contents
        .lines()
        .map(|line| line.trim())
        .any(|line| line == WORKTREE_IGNORE_ENTRY || line == WORKTREE_IGNORE_ALT_ENTRY)
}
