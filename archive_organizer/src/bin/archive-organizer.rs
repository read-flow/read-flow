use std::path::PathBuf;

use anyhow::Result;
use archive_organizer::ApplicationModule;
use archive_organizer::ScanSettingsProvider;
#[cfg(feature = "server")]
use archive_organizer::server;
use archive_organizer::settings;
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
        #[clap(long, default_value = "false")]
        dry_run: bool,
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
        Commands::Scan { dry_run, path } => {
            ApplicationModule::new(ScanSettingsProvider { dry_run }, settings::config_path())?
                .scan(path)?
        }
        #[cfg(feature = "server")]
        Commands::Serve => server::main(),
    };

    Ok(())
}
