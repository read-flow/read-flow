use relm4::RelmApp;
use tracing_subscriber::{filter::EnvFilter, fmt, prelude::*};

use archive_organizer::ApplicationModule;
use archive_organizer_gtk::{app::App, get_remote_clients};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(EnvFilter::from_default_env())
        .init();

    tracing::debug!("Starting Archive Organizer application");

    let app = RelmApp::new("net.kleinhaneveld.ArchiveOrganizer");
    tracing::debug!("Created RelmApp instance");

    let application_module = ApplicationModule::instantiate()?;
    tracing::debug!("Initialized application module");

    let db_client = application_module.db_client();
    tracing::debug!("Retrieved database client");

    let remote_file_clients = get_remote_clients(&application_module)?;
    tracing::debug!(
        "Retrieved {} remote file clients",
        remote_file_clients.len()
    );

    tracing::debug!("Starting application");
    app.run_async::<App>((db_client, remote_file_clients));

    Ok(())
}
