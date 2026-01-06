use std::path::PathBuf;

use anyhow::Result;
use archive_organizer::ApplicationModule;
#[cfg(feature = "server")]
use archive_organizer::server;
use clap::Parser;
use clap::Subcommand;
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Scan {
        path: PathBuf,
    },
    ApplyTags,
    #[cfg(feature = "server")]
    Serve,
    ExtractScanDirectories,
}

fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::ApplyTags => ApplicationModule::instantiate()?.apply_tags()?,
        Commands::ExtractScanDirectories => {
            ApplicationModule::instantiate()?.extract_scan_directories()
        }
        Commands::Scan { path } => ApplicationModule::instantiate()?.scan(path)?,
        #[cfg(feature = "server")]
        Commands::Serve => server::main(),
    };

    Ok(())
}
