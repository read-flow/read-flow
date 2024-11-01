use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tokio::runtime::Runtime;
use tracing_subscriber::{filter::EnvFilter, fmt, prelude::*};
use url::Url;

#[cfg(feature = "server")]
use archive_organizer::server;
use archive_organizer::{api::FileDataSource, client, ApplicationModule};

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
    #[cfg(feature = "gui")]
    Gui,
    #[cfg(feature = "server")]
    Serve,
    Client,
}

fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Scan { path } => ApplicationModule::instantiate()?.scan(path)?,
        #[cfg(feature = "gui")]
        Commands::Gui => ApplicationModule::instantiate()?.gui()?,
        #[cfg(feature = "server")]
        Commands::Serve => server::main(),
        Commands::Client => {
            // Create the runtime
            let rt = Runtime::new().unwrap();

            // Execute the future, blocking the current thread until completion
            rt.block_on(async {
                let client =
                    client::FilesClient::new("http://localhost:8000/".parse::<Url>().unwrap())
                        .unwrap();

                let result = client.status().await;
                tracing::info!("status result: {result:?}");

                let result = client.download_file(4, "horse-power.pdf").await;
                tracing::info!("download result: {result:?}");

                let result = client.upload_file(&PathBuf::from("horse-power.pdf")).await;
                tracing::info!("Uploaded as: {result:?}");
            });
        }
    };

    Ok(())
}
