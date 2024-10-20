use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{filter::EnvFilter, fmt, prelude::*};

use archive_organizer::{db::get_connection_pool, gui::gui, scan::scan, serve};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Scan { path: PathBuf },
    Gui,
    Serve,
}

fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Scan { path } => scan(path, get_connection_pool())?,
        Commands::Gui => gui(get_connection_pool())?,
        Commands::Serve => serve::main(),
    };

    Ok(())
}
