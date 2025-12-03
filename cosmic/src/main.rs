// SPDX-License-Identifier: GPL-3.0-or-later

mod aggregator;
mod app;
mod client;
mod component;
mod config;
mod cosmic_ext;
mod forms;
mod i18n;
mod iter;
mod page;
mod state;

use archive_organizer::ApplicationModule;
use archive_organizer::settings;
use clap::Parser;

#[derive(Debug, clap::Parser)]
pub struct Cli {
    #[clap(long, default_value = "false")]
    /// Enable private mode, which makes all `--private-tags` visible
    private_mode: bool,
    #[clap(long)]
    /// Private tags and tagged files are hidden from the UI by default
    private_tags: Vec<String>,
}

fn main() -> cosmic::iced::Result {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Get the system's preferred languages.
    let requested_languages = i18n_embed::DesktopLanguageRequester::requested_languages();

    // Enable localizations to be applied.
    i18n::init(&requested_languages);

    // Parse commandline parameters.
    let Cli {
        private_mode,
        private_tags,
    } = Cli::parse();

    // Extract settings from the application's configuration.
    let mut settings = settings::extract().expect("settings are present");
    // Merge commandline parameters with settings.
    settings.ui.merge_in((private_mode, private_tags).into());

    let application_module = ApplicationModule::from_settings(settings);

    // Settings for configuring the application window and iced runtime.
    let settings = cosmic::app::Settings::default().size_limits(
        cosmic::iced::Limits::NONE
            .min_width(360.0)
            .min_height(180.0),
    );

    // Starts the application's event loop with `()` as the application's flags.
    cosmic::app::run::<app::AppModel>(settings, application_module)
}
