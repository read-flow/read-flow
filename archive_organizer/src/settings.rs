use std::fmt;
use std::path::Path;
use std::path::PathBuf;

use figment::Figment;
use figment::providers::Format;
use figment::providers::Toml;
use indexmap::IndexMap;
use pbkdf2::Params;
use pbkdf2::Pbkdf2;
use pbkdf2::password_hash::Error as PbkdfError;
use pbkdf2::password_hash::PasswordHash;
use pbkdf2::password_hash::PasswordHasher;
use pbkdf2::password_hash::PasswordVerifier;
use pbkdf2::password_hash::SaltString;
use pbkdf2::password_hash::rand_core::OsRng;
use serde::Deserialize;
use serde::Serialize;

use crate::ExpandedPath;
use crate::db::DbSettings;
use crate::scan::ScanSettings;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Settings {
    pub database: DbSettings,
    #[serde(default)]
    pub server: ServerSettings,
    #[serde(default)]
    pub scan: ScanSettings,
    #[serde(default)]
    pub ui: UiSettings,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "server", serde(crate = "rocket::serde"))]
pub struct HashedPassword(String);

impl fmt::Display for HashedPassword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<String> for HashedPassword {
    type Error = PbkdfError;

    fn try_from(password: String) -> Result<Self, Self::Error> {
        let salt = SaltString::generate(&mut OsRng);

        let params = Params {
            rounds: 100000,
            ..Params::default()
        };
        // Hash password to PHC string ($pbkdf2-sha256$...)
        let password_hash = Pbkdf2
            .hash_password_customized(password.as_bytes(), None, None, params, &salt)?
            .to_string();
        Ok(Self(password_hash))
    }
}

impl HashedPassword {
    pub fn verify(&self, password: &str) -> Result<(), PbkdfError> {
        // Verify password against PHC string
        let parsed_hash = PasswordHash::new(&self.0)?;
        Pbkdf2.verify_password(password.as_bytes(), &parsed_hash)
    }
}

/// Settings for the `server` feature.
///
/// Exposed here so they can be edited by the cosmic application.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "server", serde(crate = "rocket::serde"))]
pub struct ServerSettings {
    pub download_folder: ExpandedPath,

    pub authorized_users: IndexMap<String, HashedPassword>,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            download_folder: Path::new("/tmp").to_path_buf().try_into().unwrap(),
            authorized_users: Default::default(),
        }
    }
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
