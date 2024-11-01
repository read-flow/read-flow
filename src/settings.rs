use figment::{
    providers::{Format, Toml},
    Error, Figment,
};
use serde::Deserialize;

use crate::db::DbSettings;
#[cfg(feature = "server")]
use crate::server::ServerSettings;

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub database: DbSettings,
    #[cfg(feature = "server")]
    pub server: ServerSettings,
}

pub fn extract() -> Result<Settings, Error> {
    let figment = Figment::new().merge(Toml::file("archive-organizer.toml"));
    figment.extract()
}
