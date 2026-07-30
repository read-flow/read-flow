// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use clap::Subcommand;
use read_flow_core::ApplicationModule;
use read_flow_core::ScanSettingsProvider;
#[cfg(feature = "server")]
use read_flow_core::server;
use read_flow_core::settings;
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
    /// Generate a local root CA and a leaf cert signed by it, for manually
    /// testing browser-trusted TLS without a public hostname. Not wired into
    /// any settings automatically — point `[server.tls]` at the printed leaf
    /// cert/key yourself, and trust the printed CA root on each test device.
    #[cfg(feature = "server")]
    GenerateTlsCa {
        /// Directory to write the CA + leaf cert/key PEM files into.
        #[clap(long, default_value = ".")]
        output_dir: PathBuf,
        /// SAN (DNS name or IP) the leaf cert should cover; repeatable.
        /// Defaults to `localhost` if none given.
        #[clap(long = "san")]
        sans: Vec<String>,
    },
    ExtractScanDirectories,
    /// Check which files in the database no longer exist on disk.
    /// With --purge, also removes stale records from the database.
    CheckMissing {
        #[clap(long, default_value = "false")]
        purge: bool,
    },
}

impl Cli {
    fn config_path(&self) -> PathBuf {
        self.configuration_file
            .clone()
            .unwrap_or_else(settings::config_path)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let config_path = cli.config_path();

    match cli.command {
        Commands::ApplyTags { dry_run, .. } => {
            ApplicationModule::new(
                ScanSettingsProvider {
                    dry_run,
                    config_path: config_path.clone(),
                },
                config_path,
            )
            .await?
            .apply_tags()
            .await?;
        }
        Commands::ExtractScanDirectories => {
            ApplicationModule::instantiate(config_path)
                .await?
                .extract_scan_directories()
                .await;
        }
        Commands::CheckMissing { purge } => {
            let missing = ApplicationModule::instantiate(config_path)
                .await?
                .check_missing(purge)
                .await;
            if missing.is_empty() {
                println!("All files in the database exist on disk.");
            } else {
                for path in &missing {
                    println!("{path}");
                }
                if purge {
                    eprintln!(
                        "Removed {} stale record(s) from the database.",
                        missing.len()
                    );
                } else {
                    eprintln!(
                        "{} file(s) missing from disk. Run with --purge to remove them from the database.",
                        missing.len()
                    );
                }
            }
        }
        Commands::Scan { dry_run, path, .. } => {
            ApplicationModule::new(
                ScanSettingsProvider {
                    dry_run,
                    config_path: config_path.clone(),
                },
                config_path,
            )
            .await?
            .scan(path)
            .await?;
        }
        #[cfg(feature = "server")]
        Commands::Serve => {
            server::main(config_path).await?;
        }
        #[cfg(feature = "server")]
        Commands::GenerateTlsCa { output_dir, sans } => {
            std::fs::create_dir_all(&output_dir)?;
            let (ca_cert, ca_key) = server::generate_local_ca(&output_dir)?;
            let (leaf_cert, leaf_key) =
                server::generate_ca_signed_cert(&output_dir, &ca_cert, &ca_key, sans)?;

            println!("CA root:   {}", ca_cert.display());
            println!("CA key:    {}", ca_key.display());
            println!("Leaf cert: {}", leaf_cert.display());
            println!("Leaf key:  {}", leaf_key.display());
            eprintln!();
            eprintln!("Next steps:");
            eprintln!(
                "1. In read-flow.toml, set [server.tls] cert = \"{}\" and key = \"{}\", then restart the server.",
                leaf_cert.display(),
                leaf_key.display()
            );
            eprintln!(
                "2. Trust {} once on each client device you want to test from:",
                ca_cert.display()
            );
            eprintln!(
                "   macOS: open the file (Keychain Access), double-click the imported cert, expand Trust, set \"When using this certificate\" to Always Trust."
            );
            eprintln!(
                "   Then visit https://<address>:<port> from that device — no warning, and the PWA should be installable."
            );
        }
    };

    Ok(())
}
