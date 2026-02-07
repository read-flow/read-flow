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
    #[clap(long)]
    /// Path to the configuration file to use instead of the default
    configuration_file: Option<PathBuf>,
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
    ApplyTags {
        #[clap(long, default_value = "false")]
        dry_run: bool,
    },
    #[cfg(feature = "server")]
    Serve,
    ExtractScanDirectories,
}

impl Cli {
    fn config_path(&self) -> PathBuf {
        self.configuration_file
            .clone()
            .unwrap_or_else(settings::config_path)
    }
}

fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let config_path = cli.config_path();

    match cli.command {
        Commands::ApplyTags { dry_run, .. } => {
            ApplicationModule::new(ScanSettingsProvider { dry_run }, config_path)?.apply_tags()?;
        }
        Commands::ExtractScanDirectories => {
            ApplicationModule::instantiate(config_path)?.extract_scan_directories();
        }
        Commands::Scan { dry_run, path, .. } => {
            ApplicationModule::new(ScanSettingsProvider { dry_run }, config_path)?.scan(path)?;
        }
        #[cfg(feature = "server")]
        Commands::Serve => {
            server::main(config_path)?;
        }
    };

    Ok(())
}
