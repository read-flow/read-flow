use clap::Parser;

use archive_organizer::{
    settings::{self, UiSettings},
    ApplicationModule,
};
use iced_gui::run_gui;

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
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Parse commandline arguments
    let cli = Cli::parse();

    // Load settings
    let mut settings = settings::extract()?;

    // Merge commandline arguments into settings
    settings.ui.merge_in(cli.into());

    // Initialize the application module
    let application_module = ApplicationModule::from_settings(settings);

    // Run the GUI
    run_gui(application_module)?;

    Ok(())
}
