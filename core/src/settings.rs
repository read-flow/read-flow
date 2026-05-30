use std::fmt;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use figment::Figment;
use figment::providers::Format;
use figment::providers::Toml;
use indexmap::IndexMap;
use pbkdf2::PasswordHasher;
use pbkdf2::PasswordVerifier;
use pbkdf2::Pbkdf2;
use pbkdf2::password_hash::Error as PbkdfError;
use pbkdf2::phc::PasswordHash;
use rand_core::OsRng;
use rand_core::RngCore;
use serde::Deserialize;
use serde::Serialize;

use crate::ExpandedPath;
use crate::db::DbSettings;
use crate::online_library::OnlineCatalog;
use crate::scan::ScanSettings;

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize, Serialize)]
pub struct Settings {
    #[serde(default)]
    pub database: DbSettings,
    #[serde(default)]
    pub client: ClientSettings,
    #[serde(default)]
    pub server: ServerSettings,
    #[serde(default)]
    pub scan: ScanSettings,
    #[serde(default)]
    pub ui: UiSettings,
    #[serde(default)]
    pub online_library: OnlineLibrarySettings,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct OnlineLibrarySettings {
    pub catalogs: Vec<OnlineCatalog>,
}

impl Default for OnlineLibrarySettings {
    fn default() -> Self {
        Self {
            catalogs: vec![
                OnlineCatalog::project_gutenberg(),
                OnlineCatalog::standard_ebooks(),
            ],
        }
    }
}

impl Settings {
    pub fn extract() -> Result<Self, SettingsError> {
        let figment = {
            let figment = Figment::new();
            decorate_with(figment, config_path())
        };
        Self::from_figment(figment)
    }

    pub fn extract_from(path: &Path) -> Result<Self, SettingsError> {
        let figment = decorate_with(Figment::new(), path.to_path_buf());
        Self::from_figment(figment)
    }

    /// Save settings to the configuration file
    pub fn save(&self, path: &Path) -> Result<(), SettingsError> {
        let toml_string = toml::to_string_pretty(self)?;
        fs::write(path, toml_string)?;
        Ok(())
    }

    fn from_figment(figment: Figment) -> Result<Settings, SettingsError> {
        let settings: Option<Settings> = figment.extract()?;
        Ok(settings.unwrap_or_default())
    }
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
        let mut salt = [0u8; 16];
        OsRng.fill_bytes(&mut salt);
        let password_hash = Pbkdf2::default()
            .hash_password_with_salt(password.as_bytes(), &salt)?
            .to_string();
        Ok(Self(password_hash))
    }
}

impl HashedPassword {
    pub fn verify(&self, password: &str) -> Result<(), PbkdfError> {
        let parsed_hash = PasswordHash::new(&self.0).map_err(PbkdfError::from)?;
        Pbkdf2::default().verify_password(password.as_bytes(), &parsed_hash)
    }
}

/// Settings for the HTTP client (remote file downloads).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ClientSettings {
    pub download_folder: ExpandedPath,
}

impl Default for ClientSettings {
    fn default() -> Self {
        Self {
            download_folder: expanduser::expanduser("~/Downloads")
                .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
                .try_into()
                .unwrap(),
        }
    }
}

/// A user entry in `authorized_users`. Accepts either a plain password string
/// (legacy) or an extended form with optional roles.
///
/// ```toml
/// # legacy — no roles
/// guest = "$pbkdf2-sha256$..."
/// # extended — with owner role
/// alice = { password = "$pbkdf2-sha256$...", roles = ["owner"] }
/// ```
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "server", serde(crate = "rocket::serde"))]
#[serde(untagged)]
pub enum UserEntry {
    Simple(HashedPassword),
    Extended {
        password: HashedPassword,
        #[serde(default)]
        roles: Vec<String>,
    },
}

impl UserEntry {
    pub fn password(&self) -> &HashedPassword {
        match self {
            UserEntry::Simple(p) => p,
            UserEntry::Extended { password, .. } => password,
        }
    }

    pub fn roles(&self) -> &[String] {
        match self {
            UserEntry::Simple(_) => &[],
            UserEntry::Extended { roles, .. } => roles,
        }
    }

    pub fn has_role(&self, role: &str) -> bool {
        self.roles().iter().any(|r| r == role)
    }
}

/// Settings for the `server` feature.
///
/// Exposed here so they can be edited by the cosmic application.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "server", serde(crate = "rocket::serde"))]
pub struct ServerSettings {
    pub download_folder: ExpandedPath,

    pub authorized_users: IndexMap<String, UserEntry>,
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
    if Path::new("Cargo.toml").exists() && Path::new("read-flow.toml").exists() {
        PathBuf::from("read-flow.toml")
            .canonicalize()
            .expect("should work for valid file")
    } else {
        expanduser::expanduser("~/.config/read-flow/read-flow.toml")
            .expect("could not expand user home")
    }
}

pub fn decorate_with(figment: Figment, path: PathBuf) -> Figment {
    if !path.exists() {
        crate::force_create(&path);
        tracing::warn!(
            "No configuration file found, created empty one at: `{}`",
            path.display()
        );
    }

    tracing::info!("using configuration from `{}`", path.display());
    figment.merge(Toml::file(path))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_entry_simple_parses_from_plain_string() {
        let toml = r#"
[server]
download_folder = "/tmp"

[server.authorized_users]
guest = "$pbkdf2-sha256$i=100000,l=32$abc$def"
"#;
        let settings: Settings = toml::from_str(toml).unwrap();
        let entry = settings.server.authorized_users.get("guest").unwrap();
        assert!(matches!(entry, UserEntry::Simple(_)));
        assert_eq!(entry.roles(), &[] as &[String]);
        assert!(!entry.has_role("owner"));
    }

    #[test]
    fn user_entry_extended_parses_with_roles() {
        let toml = r#"
[server]
download_folder = "/tmp"

[server.authorized_users]
alice = { password = "$pbkdf2-sha256$i=100000,l=32$abc$def", roles = ["owner"] }
"#;
        let settings: Settings = toml::from_str(toml).unwrap();
        let entry = settings.server.authorized_users.get("alice").unwrap();
        assert!(entry.has_role("owner"));
        assert!(!entry.has_role("admin"));
    }

    #[test]
    fn online_library_settings_default_round_trips_through_toml() {
        let original = OnlineLibrarySettings::default();
        let serialized = toml::to_string(&original).unwrap();
        let deserialized: OnlineLibrarySettings = toml::from_str(&serialized).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn settings_missing_online_library_section_uses_default() {
        let settings: Settings = toml::from_str("").unwrap();
        assert!(
            !settings.online_library.catalogs.is_empty(),
            "default catalogs should be populated"
        );
    }

    #[test]
    fn online_library_default_includes_project_gutenberg() {
        let settings = OnlineLibrarySettings::default();
        assert!(
            settings
                .catalogs
                .iter()
                .any(|c| c.name.contains("Gutenberg")),
            "default catalogs should include Project Gutenberg"
        );
    }
}
