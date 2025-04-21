use clap::Parser;
use relm4::RelmApp;
use tracing_subscriber::{filter::EnvFilter, fmt, prelude::*};

use archive_organizer::{
    ApplicationModule,
    settings::{self, UiSettings},
};
use archive_organizer_gtk::app::App;

#[derive(Debug, clap::Parser)]
pub struct Cli {
    #[clap(long, default_value = "false")]
    /// Enable private mode, which makes all `--private-tags` visible
    private_mode: bool,
    #[clap(long)]
    /// Private tags and tagged files are hidden from the UI by default
    private_tags: Vec<String>,
}

impl From<Cli> for UiSettings {
    fn from(source: Cli) -> Self {
        Self::new(source.private_mode, source.private_tags)
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(EnvFilter::from_default_env())
        .init();

    tracing::debug!("Starting Archive Organizer application");

    let cli = Cli::parse();
    tracing::debug!("Parsed commandline arguments");

    let mut settings = settings::extract()?;
    tracing::debug!("Loaded settings");

    settings.ui.merge_in(cli.into());
    tracing::debug!("Merged CLI arguments into settings");

    let application_module = ApplicationModule::from_settings(settings);
    tracing::debug!("Initialized application module");

    let app = RelmApp::new("net.kleinhaneveld.ArchiveOrganizer");
    tracing::debug!("Created RelmApp instance");

    tracing::debug!("Starting application");
    app.run_async::<App>(application_module);

    Ok(())
}
