use clap::{Parser, Subcommand};

use crate::{
    Repo,
    commands::{create::CreateCommand, list::ListCommand},
};

#[derive(Parser, Debug)]
#[command(name = "git-worktree-helper", version, about = "Manage Git worktrees more easily", long_about = None)]
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
}

#[derive(Parser, Debug)]
struct CreateArgs {
    /// Name of the worktree (also used as the branch name)
    name: String,
}

pub fn run() -> color_eyre::Result<()> {
    let cli = Cli::parse();
    let repo = Repo::discover()?;

    match cli.command {
        Commands::Create(args) => {
            let command = CreateCommand::new(args.name);
            command.execute(&repo)?;
        }
        Commands::Ls => {
            let command = ListCommand::default();
            command.execute(&repo)?;
        }
    }

    Ok(())
}
