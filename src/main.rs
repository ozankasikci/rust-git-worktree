use rsworktree::cli;

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    cli::run()
}
