use std::path::{Path, PathBuf};

use figment::{
    providers::{Format, Toml},
    Error, Figment,
};
use serde::Deserialize;

use crate::db::DbSettings;
use crate::scan::ScanSettings;
#[cfg(feature = "server")]
use crate::server::ServerSettings;

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub database: DbSettings,
    #[cfg(feature = "server")]
    pub server: ServerSettings,
    pub scan: Option<ScanSettings>,
}

pub fn decorate(figment: Figment) -> Figment {
    let path = if Path::new("Cargo.toml").exists() && Path::new("archive-organizer.toml").exists() {
        let path = PathBuf::from("archive-organizer.toml")
            .canonicalize()
            .expect("should work for valid file");
        tracing::warn!(
            "detected `archive-organizer.toml` and `Cargo.toml` in current directory, loading `{}`",
            path.display()
        );

        path
    } else {
        let path = expanduser::expanduser("~/.config/archive-organizer/archive-organizer.toml")
            .expect("could not expand user home");

        if !path.exists() {
            tracing::error!(
                "No configuration file found, please create one in: `{}`",
                path.display()
            );
            panic!("No configuration file found");
        } else {
            tracing::info!("using configuration from `{}`", path.display());
        }

        path
    };
    figment.merge(Toml::file(path))
}

pub fn extract() -> Result<Settings, Error> {
    let figment = decorate(Figment::new());
    figment.extract()
}
