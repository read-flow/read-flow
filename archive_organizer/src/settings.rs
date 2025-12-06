use std::path::Path;
use std::path::PathBuf;

use figment::Figment;
use figment::providers::Format;
use figment::providers::Toml;
use serde::Deserialize;
use serde::Serialize;

use crate::db::DbSettings;
use crate::scan::ScanSettings;
#[cfg(feature = "server")]
use crate::server::ServerSettings;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Settings {
    pub database: DbSettings,
    #[cfg(feature = "server")]
    pub server: ServerSettings,
    #[serde(default)]
    pub scan: ScanSettings,
    #[serde(default)]
    pub ui: UiSettings,
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("configuration error: {0}")]
    Figment(#[source] Box<figment::Error>),
    #[error("serialization error: {0}")]
    Toml(#[from] toml::ser::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl From<figment::Error> for SettingsError {
    fn from(source: figment::Error) -> Self {
        SettingsError::Figment(Box::new(source))
    }
}

/// Get the path to the configuration file
pub fn config_path() -> PathBuf {
    if Path::new("Cargo.toml").exists() && Path::new("archive-organizer.toml").exists() {
        PathBuf::from("archive-organizer.toml")
            .canonicalize()
            .expect("should work for valid file")
    } else {
        expanduser::expanduser("~/.config/archive-organizer/archive-organizer.toml")
            .expect("could not expand user home")
    }
}

pub fn decorate(figment: Figment) -> Figment {
    let path = config_path();

    if Path::new("Cargo.toml").exists() && Path::new("archive-organizer.toml").exists() {
        tracing::warn!(
            "detected `archive-organizer.toml` and `Cargo.toml` in current directory, loading `{}`",
            path.display()
        );
    } else if !path.exists() {
        tracing::error!(
            "No configuration file found, please create one in: `{}`",
            path.display()
        );
        panic!("No configuration file found");
    } else {
        tracing::info!("using configuration from `{}`", path.display());
    }

    figment.merge(Toml::file(path))
}

pub fn extract() -> Result<Settings, SettingsError> {
    let figment = decorate(Figment::new());
    let settings = figment.extract()?;
    Ok(settings)
}

/// Save settings to the configuration file
pub fn save(settings: &Settings) -> Result<(), SettingsError> {
    let path = config_path();
    let toml_string = toml::to_string_pretty(settings)?;
    std::fs::write(path, toml_string)?;
    Ok(())
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct UiSettings {
    #[serde(default)]
    private_mode: bool,
    #[serde(default)]
    private_tags: Vec<String>,
}

impl From<(bool, Vec<String>)> for UiSettings {
    fn from((private_mode, private_tags): (bool, Vec<String>)) -> Self {
        Self::new(private_mode, private_tags)
    }
}

impl UiSettings {
    pub fn new(private_mode: bool, private_tags: Vec<String>) -> Self {
        Self {
            private_mode,
            private_tags,
        }
    }

    pub fn private_mode(&self) -> bool {
        self.private_mode
    }

    pub fn set_private_mode(&mut self, private_mode: bool) {
        self.private_mode = private_mode;
    }

    pub fn private_tags(&self) -> &[String] {
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
