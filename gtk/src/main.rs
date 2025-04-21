use clap::Parser;
use relm4::RelmApp;
use tracing_subscriber::{filter::EnvFilter, fmt, prelude::*};

use archive_organizer::{
    ApplicationModule,
    settings::{self},
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

    /// Unknown arguments or everything after -- gets passed through to GTK.
    #[arg(allow_hyphen_values = true, trailing_var_arg = true)]
    gtk_options: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(EnvFilter::from_default_env())
        .init();

    tracing::debug!("Starting Archive Organizer application");

    let Cli {
        private_mode,
        private_tags,
        gtk_options,
    } = Cli::parse();
    tracing::debug!("Parsed commandline arguments");

    let mut settings = settings::extract()?;
    tracing::debug!("Loaded settings");

    settings.ui.merge_in((private_mode, private_tags).into());
    tracing::debug!("Merged CLI arguments into settings");

    let application_module = ApplicationModule::from_settings(settings);
    tracing::debug!("Initialized application module");

    let program_invocation = std::env::args().next().unwrap();
    let mut gtk_args = vec![program_invocation];
    gtk_args.extend(gtk_options.clone());

    let app = RelmApp::new("net.kleinhaneveld.ArchiveOrganizer").with_args(gtk_args);
    tracing::debug!("Created RelmApp instance: {app:?}");

    tracing::debug!("Starting application");
    app.run_async::<App>(application_module);

    Ok(())
}
