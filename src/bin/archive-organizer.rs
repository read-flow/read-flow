use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use tokio::runtime::Runtime;
use tracing_subscriber::{filter::EnvFilter, fmt, prelude::*};
use url::Url;

#[cfg(feature = "gui")]
use archive_organizer::gui::gui;
use archive_organizer::{
    client,
    db::{get_connection_pool, ConnectionPool},
    scan::scan,
    serve,
};

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

fn run_migrations(connection_pool: &ConnectionPool) -> Result<()> {
    let mut connection = connection_pool.get()?;

    // This will run the necessary migrations.
    // See the documentation for `MigrationHarness` for all available methods.
    // TODO: error handling
    let _ = connection.run_pending_migrations(MIGRATIONS);

    Ok(())
}

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
    Serve,
    Client,
}

fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    let connection_pool = get_connection_pool();
    run_migrations(&connection_pool)?;

    match cli.command {
        Commands::Scan { path } => scan(path, get_connection_pool())?,
        #[cfg(feature = "gui")]
        Commands::Gui => gui(get_connection_pool())?,
        Commands::Serve => serve::main(),
        Commands::Client => {
            // Create the runtime
            let rt = Runtime::new().unwrap();

            // Execute the future, blocking the current thread until completion
            rt.block_on(async {
                let client =
                    client::FilesClient::new("http://localhost:8000/".parse::<Url>().unwrap())
                        .unwrap();

                client.download_file(4, "horse-power.pdf").await.unwrap();

                let result = client.upload_file(&PathBuf::from("horse-power.pdf")).await;

                tracing::info!("Uploaded as: {result:?}");
            });
        }
    };

    Ok(())
}
