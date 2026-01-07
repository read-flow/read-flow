// SPDX-License-Identifier: GPL-3.0-or-later

mod aggregator;
mod app;
mod client;
mod component;
mod config;
mod cosmic_ext;
mod document_provider;
mod forms;
mod i18n;
mod iter;
mod page;
mod state;

use std::sync::Arc;

use archive_organizer::ApplicationModule as GenericApplicationModule;
use archive_organizer::settings;
use archive_organizer::settings::Settings;
use archive_organizer::settings::SettingsError;
use clap::Parser;
use provider::sync::Provider;

const ICON_SIZE: u16 = 16;

pub type ApplicationModule = GenericApplicationModule<AppSettings>;

#[derive(Debug, clap::Parser)]
pub struct Cli {
    #[clap(long, default_value = "false")]
    /// Enable private mode, which makes all `--private-tags` visible
    private_mode: bool,
    #[clap(long)]
    /// Private tags and tagged files are hidden from the UI by default
    private_tags: Vec<String>,
}

pub struct AppSettings {
    cli_parameters: Cli,
}

impl Provider<Settings> for AppSettings {
    type Error = SettingsError;
    fn provide(&self) -> Result<Settings, Self::Error> {
        let Cli {
            private_mode,
            private_tags,
        } = &self.cli_parameters;

        // Extract settings from the application's configuration.
        let mut settings = settings::extract().expect("settings are present");
        // Merge commandline parameters with settings.
        settings
            .ui
            .merge_in((*private_mode, private_tags.clone()).into());

        Ok(settings)
    }
}

fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Get the system's preferred languages.
    let requested_languages = i18n_embed::DesktopLanguageRequester::requested_languages();

    // Enable localizations to be applied.
    i18n::init(&requested_languages);

    // Parse commandline parameters.
    let settings = AppSettings {
        cli_parameters: Cli::parse(),
    };

    let application_module = Arc::new(ApplicationModule::new(settings)?);

    // Settings for configuring the application window and iced runtime.
    let settings = cosmic::app::Settings::default().size_limits(
        cosmic::iced::Limits::NONE
            .min_width(360.0)
            .min_height(180.0),
    );

    // Starts the application's event loop with `()` as the application's flags.
    cosmic::app::run::<app::AppModel>(settings, application_module)?;

    Ok(())
}
