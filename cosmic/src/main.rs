// SPDX-License-Identifier: MIT

mod app;
mod config;
mod i18n;

use archive_organizer::{
    settings::{self},
    ApplicationModule,
};

fn main() -> cosmic::iced::Result {
    // Get the system's preferred languages.
    let requested_languages = i18n_embed::DesktopLanguageRequester::requested_languages();

    // Enable localizations to be applied.
    i18n::init(&requested_languages);

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
