use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use color_eyre::eyre::{self, Context};
use git2::Repository as GitRepository;

const WORKTREE_IGNORE_ENTRY: &str = ".rsworktree/";
const WORKTREE_IGNORE_ALT_ENTRY: &str = ".rsworktree";

pub struct Repo {
    git: GitRepository,
    root: PathBuf,
}

impl std::fmt::Debug for Repo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Repo").field("root", &self.root).finish()
    }
}

impl Repo {
    pub fn discover() -> color_eyre::Result<Self> {
        let cwd = std::env::current_dir().wrap_err("failed to read current directory")?;
        Self::discover_from(&cwd)
    }

    pub fn discover_from<P: AsRef<Path>>(path: P) -> color_eyre::Result<Self> {
        let discovered =
            GitRepository::discover(path.as_ref()).wrap_err("failed to discover git repository")?;

        let common_dir = discovered.commondir().to_path_buf();
        let root = common_dir
            .parent()
            .ok_or_else(|| {
                eyre::eyre!(
                    "failed to determine repository root from `{}`",
                    common_dir.display()
                )
            })?
            .to_path_buf();

        let git = if discovered.is_worktree() {
            GitRepository::open(&root)
                .or_else(|_| GitRepository::open(common_dir.clone()))
                .wrap_err("failed to open parent repository for worktree")?
        } else {
            discovered
        };

        Ok(Self { git, root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn git(&self) -> &GitRepository {
        &self.git
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use tempfile::TempDir;

    fn init_repo(dir: &TempDir) -> color_eyre::Result<Repo> {
        git2::Repository::init(dir.path())?;
        Repo::discover_from(dir.path())
    }

    #[test]
    fn ensure_worktrees_dir_creates_gitignore_when_missing() -> color_eyre::Result<()> {
        let dir = TempDir::new()?;
        let repo = init_repo(&dir)?;
        let gitignore = dir.path().join(".gitignore");

        assert!(!gitignore.exists(), "gitignore should start absent");

        repo.ensure_worktrees_dir()?;

        let contents = fs::read_to_string(&gitignore)?;
        assert_eq!(contents, format!("{WORKTREE_IGNORE_ENTRY}\n"));

        Ok(())
    }

    #[test]
    fn ensure_worktrees_dir_appends_entry_once() -> color_eyre::Result<()> {
        let dir = TempDir::new()?;
        let repo = init_repo(&dir)?;
        let gitignore = dir.path().join(".gitignore");
        fs::write(&gitignore, "target")?;

        repo.ensure_worktrees_dir()?;
        let contents = fs::read_to_string(&gitignore)?;
        assert_eq!(contents, format!("target\n{WORKTREE_IGNORE_ENTRY}\n"));

        repo.ensure_worktrees_dir()?;
        let contents_again = fs::read_to_string(&gitignore)?;
        assert_eq!(contents_again, contents);

        Ok(())
    }

    #[test]
    fn gitignore_has_entry_detects_alternate_form() {
        assert!(gitignore_has_entry(".rsworktree\n"));
        assert!(gitignore_has_entry("  .rsworktree/  \n"));
        assert!(!gitignore_has_entry(".other"));
    }

    #[test]
    fn ensure_worktrees_dir_respects_existing_entry() -> color_eyre::Result<()> {
        let dir = TempDir::new()?;
        let repo = init_repo(&dir)?;
        let gitignore = dir.path().join(".gitignore");
        fs::write(&gitignore, format!("{WORKTREE_IGNORE_ALT_ENTRY}\n"))?;

        repo.ensure_worktrees_dir()?;

        let contents = fs::read_to_string(&gitignore)?;
        assert_eq!(contents, format!("{WORKTREE_IGNORE_ALT_ENTRY}\n"));

        Ok(())
    }
}
