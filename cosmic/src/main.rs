// SPDX-License-Identifier: GPL-3.0-or-later

mod aggregator;
mod app;
mod client;
mod component;
mod config;
mod cosmic_ext;
mod document_provider;
mod fonts;
mod forms;
mod i18n;
mod iter;
mod layout;
mod page;
mod state;

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use provider::sync::Provider;
use read_flow_core::ApplicationModule as GenericApplicationModule;
use read_flow_core::settings;
use read_flow_core::settings::Settings;
use read_flow_core::settings::SettingsError;

const ICON_SIZE: u16 = 16;

pub type ApplicationModule = GenericApplicationModule<AppSettings>;

#[derive(Debug, clap::Parser)]
pub struct Cli {
    #[clap(long)]
    /// Path to the configuration file to use instead of the default
    configuration_file: Option<PathBuf>,
    #[clap(long, default_value = "false")]
    /// Enable private mode, which makes all `--private-tags` visible
    private_mode: bool,
    #[clap(long)]
    /// Private tags and tagged files are hidden from the UI by default
    private_tags: Vec<String>,
}

#[derive(Debug)]
pub struct AppSettings {
    cli_parameters: Cli,
}

impl AppSettings {
    fn config_path(&self) -> PathBuf {
        self.cli_parameters
            .configuration_file
            .clone()
            .unwrap_or_else(settings::config_path)
    }
}

impl Provider<Settings> for AppSettings {
    type Error = SettingsError;
    fn provide(&self) -> Result<Settings, Self::Error> {
        let Cli {
            configuration_file,
            private_mode,
            private_tags,
        } = &self.cli_parameters;

        // Extract settings from the application's configuration.
        let mut settings = match configuration_file {
            Some(path) => Settings::extract_from(path).expect("settings are present"),
            None => Settings::extract().expect("settings are present"),
        };
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

    let config_path = settings.config_path();
    let application_module = Arc::new(ApplicationModule::new(settings, config_path)?);

    // Settings for configuring the application window and iced runtime.
    let settings = cosmic::app::Settings::default().size_limits(
        cosmic::iced::Limits::NONE
            .min_width(360.0)
            .min_height(180.0),
    );

    // Starts the application's event loop with `()` as the application's flags.
    cosmic::app::run::<app::ReadFlow>(settings, application_module)?;

    Ok(())
}
