use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use archive_organizer::commands;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Scan { directory: PathBuf },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    commands::init();

    match cli.command {
        Commands::Scan { directory } => commands::scan::scan(&directory),
    }
}
