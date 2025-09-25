use git_worktree_helper::cli;

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    cli::run()
}
