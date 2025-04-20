use relm4::RelmApp;
use tracing_subscriber::{filter::EnvFilter, fmt, prelude::*};

use archive_organizer::ApplicationModule;
use archive_organizer_gtk::app::App;

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

    tracing::debug!("Starting application");
    app.run_async::<App>(application_module);

    Ok(())
}
