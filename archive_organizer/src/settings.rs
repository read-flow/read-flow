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
    pub scan: ScanSettings,
    pub ui: UiSettings,
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

#[derive(Debug, Deserialize)]
pub struct UiSettings {
    #[serde(default)]
    private_mode: bool,
    private_tags: Vec<String>,
}

impl UiSettings {
    pub fn new(private_mode: bool, private_tags: Vec<String>) -> Self {
        Self {
            private_mode,
            private_tags,
        }
    }

    pub fn get_private_mode(&self) -> bool {
        self.private_mode
    }

    pub fn set_private_mode(&mut self, private_mode: bool) {
        self.private_mode = private_mode;
    }

    pub fn get_private_tags(&self) -> &[String] {
        &self.private_tags
    }

    pub fn set_private_tags(&mut self, private_tags: Vec<String>) {
        self.private_tags = private_tags;
    }

    pub fn contains_hidden_tag(&self, tags: &[String]) -> bool {
        if self.private_mode {
            false
        } else {
            tags.iter().any(|tag| self.private_tags.contains(tag))
        }
    }

    pub fn hidden_tags(&self) -> &[String] {
        if self.private_mode {
            &[]
        } else {
            self.private_tags.as_slice()
        }
    }

    pub fn merge_in(&mut self, other: Self) {
        self.private_mode |= other.private_mode;
        self.private_tags.extend(other.private_tags);
    }
}
