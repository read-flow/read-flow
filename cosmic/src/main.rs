// SPDX-License-Identifier: GPL-3.0-or-later

mod aggregator;
mod app;
mod client;
mod component;
mod config;
mod cosmic_ext;
mod i18n;
mod iter;
mod page;
mod state;

use archive_organizer::{
    ApplicationModule,
    settings::{self},
};

fn main() -> cosmic::iced::Result {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Get the system's preferred languages.
    let requested_languages = i18n_embed::DesktopLanguageRequester::requested_languages();

    // Enable localizations to be applied.
    i18n::init(&requested_languages);

    // Extract settings from the application's configuration.
    let settings = settings::extract().expect("settings are present");
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
