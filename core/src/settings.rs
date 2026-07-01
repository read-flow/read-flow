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

    #[cfg(feature = "test-support")]
    pub fn with_rounds(password: &str, rounds: u32) -> Result<Self, PbkdfError> {
        use pbkdf2::Params;
        use pbkdf2::password_hash::CustomizedPasswordHasher;
        let mut salt = [0u8; 16];
        OsRng.fill_bytes(&mut salt);
        let params = Params::new(rounds)?;
        let password_hash = Pbkdf2::default()
            .hash_password_customized(password.as_bytes(), &salt, None, None, params)?
            .to_string();
        Ok(Self(password_hash))
    }
}

/// Settings for the HTTP client (remote file downloads).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ClientSettings {
    pub download_folder: ExpandedPath,
}

fn default_download_dir() -> PathBuf {
    directories::UserDirs::new()
        .and_then(|u| u.download_dir().map(|p| p.to_path_buf()))
        .unwrap_or_else(std::env::temp_dir)
}

impl Default for ClientSettings {
    fn default() -> Self {
        Self {
            download_folder: default_download_dir().try_into().unwrap(),
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
pub struct ServerSettings {
    pub download_folder: ExpandedPath,

    pub authorized_users: IndexMap<String, UserEntry>,

    /// Address the HTTP server binds to. Defaults to `127.0.0.1`. Overridable
    /// at runtime by the `READ_FLOW_ADDRESS` environment variable.
    #[serde(default)]
    pub address: Option<String>,

    /// Port the HTTP server binds to. Defaults to `8000`. `0` requests an
    /// OS-assigned port. Overridable by the `READ_FLOW_PORT` environment
    /// variable.
    #[serde(default)]
    pub port: Option<u16>,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            download_folder: std::env::temp_dir().try_into().unwrap(),
            authorized_users: Default::default(),
            address: None,
            port: None,
        }
    }
}

impl ServerSettings {
    /// Resolve the socket address the server should bind to. Environment
    /// variables (`READ_FLOW_ADDRESS`, `READ_FLOW_PORT`) take precedence over
    /// the configured values, which in turn fall back to `127.0.0.1:8000`.
    #[cfg(feature = "server")]
    pub fn bind_addr(&self) -> std::net::SocketAddr {
        let env_addr = std::env::var("READ_FLOW_ADDRESS").ok();
        let env_port = std::env::var("READ_FLOW_PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok());
        resolve_bind_addr(
            self.address.as_deref(),
            self.port,
            env_addr.as_deref(),
            env_port,
        )
    }
}

/// Pure resolution of the bind address from the four possible sources, in
/// precedence order: env var, then configured value, then default.
#[cfg(feature = "server")]
fn resolve_bind_addr(
    cfg_addr: Option<&str>,
    cfg_port: Option<u16>,
    env_addr: Option<&str>,
    env_port: Option<u16>,
) -> std::net::SocketAddr {
    use std::net::IpAddr;
    use std::net::Ipv4Addr;

    let ip: IpAddr = env_addr
        .or(cfg_addr)
        .and_then(|a| a.parse().ok())
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    let port = env_port.or(cfg_port).unwrap_or(8000);
    std::net::SocketAddr::new(ip, port)
}

#[cfg(all(test, feature = "server"))]
mod bind_addr_tests {
    use std::net::SocketAddr;

    use rstest::rstest;

    use super::resolve_bind_addr;

    #[rstest]
    // defaults when nothing is set
    #[case(None, None, None, None, "127.0.0.1:8000")]
    // configured values are used
    #[case(Some("0.0.0.0"), Some(9000), None, None, "0.0.0.0:9000")]
    // env overrides config
    #[case(
        Some("0.0.0.0"),
        Some(9000),
        Some("127.0.0.1"),
        Some(3000),
        "127.0.0.1:3000"
    )]
    // env port only, config address only
    #[case(Some("0.0.0.0"), None, None, Some(0), "0.0.0.0:0")]
    // invalid address falls back to default ip, keeps port
    #[case(Some("not-an-ip"), Some(1234), None, None, "127.0.0.1:1234")]
    fn resolves(
        #[case] cfg_addr: Option<&str>,
        #[case] cfg_port: Option<u16>,
        #[case] env_addr: Option<&str>,
        #[case] env_port: Option<u16>,
        #[case] expected: &str,
    ) {
        let expected: SocketAddr = expected.parse().unwrap();
        assert_eq!(
            resolve_bind_addr(cfg_addr, cfg_port, env_addr, env_port),
            expected
        );
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
        directories::ProjectDirs::from("", "", "read-flow")
            .map(|d| d.config_dir().join("read-flow.toml"))
            .unwrap_or_else(|| PathBuf::from("read-flow.toml"))
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
