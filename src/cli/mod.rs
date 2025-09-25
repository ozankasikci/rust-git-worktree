use clap::{Parser, Subcommand};

use crate::{
    Repo,
    commands::{cd::CdCommand, create::CreateCommand, list::ListCommand, rm::RemoveCommand},
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
    /// Only print the resolved path instead of spawning a shell
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
    }

    Ok(())
}
