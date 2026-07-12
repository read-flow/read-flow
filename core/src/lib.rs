// SPDX-License-Identifier: AGPL-3.0-or-later

pub mod api;
pub mod client;
pub mod db;
pub mod online_library;
pub mod scan;
#[cfg(feature = "server")]
pub mod server;
pub mod settings;
pub mod tag;
#[cfg(feature = "test-support")]
pub mod test_support;

use std::fmt;
use std::fs;
use std::hash::Hash;
use std::io;
use std::ops::Deref;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use api::FileDataSource;
use db::ConnectionPool;
use db::dao;
use db::datasource::DbClient;
use db::datasource::FilteredDbClient;
use indexmap::IndexMap;
use itertools::Itertools;
use provider::r#async::HasSetExpired;
use provider::r#async::Invalidated;
use provider::r#async::Observable;
use provider::r#async::ObservableCache;
use provider::r#async::Provider;
use rustc_hash::FxBuildHasher;
use scan::DirectorySettings;
use serde::Deserialize;
use serde::Serialize;
use settings::Settings;
use settings::SettingsError;
use sha2::Digest;
use sha2::Sha256;
use tokio::io::AsyncReadExt;
use tokio::sync::broadcast;

type SettingsCache<P> = ObservableCache<P, fn(Settings) -> Settings, Settings, Settings>;
type ConnectionPoolCache<P> = ObservableCache<
    ConnectionPoolProvider<P>,
    fn(ConnectionPool) -> ConnectionPool,
    ConnectionPool,
    ConnectionPool,
>;
type ClientCache<P> = ObservableCache<
    Arc<ConnectionPoolCache<P>>,
    fn(ConnectionPool) -> DbClient,
    ConnectionPool,
    DbClient,
>;

/// Async bridge: fetches Settings from the cache and creates a fresh ConnectionPool.
struct ConnectionPoolProvider<P> {
    settings_cache: Arc<SettingsCache<P>>,
}

impl<P> Provider<ConnectionPool> for ConnectionPoolProvider<P>
where
    P: Provider<Settings, Error = SettingsError> + Send + Sync,
{
    type Error = SettingsError;
    async fn provide(&self) -> Result<ConnectionPool, Self::Error> {
        let settings = self.settings_cache.provide().await?;
        Ok(db::get_connection_pool(&settings.database).await)
    }
}

#[derive(Debug)]
pub struct ApplicationModule<P> {
    config_path: PathBuf,
    settings: Arc<SettingsCache<P>>,
    connection_pool: Arc<ConnectionPoolCache<P>>,
    db_client: Arc<ClientCache<P>>,
}

pub struct SettingsProvider {
    pub config_path: PathBuf,
}

impl Provider<Settings> for SettingsProvider {
    type Error = SettingsError;
    async fn provide(&self) -> Result<Settings, Self::Error> {
        Settings::extract_from(&self.config_path)
    }
}

pub struct ScanSettingsProvider {
    pub dry_run: bool,
    pub config_path: PathBuf,
}

impl Provider<Settings> for ScanSettingsProvider {
    type Error = SettingsError;
    async fn provide(&self) -> Result<Settings, Self::Error> {
        let mut settings = Settings::extract_from(&self.config_path)?;
        settings.scan.merge_dry_run(self.dry_run);
        Ok(settings)
    }
}

impl ApplicationModule<SettingsProvider> {
    pub async fn instantiate(config_path: PathBuf) -> Result<Self, SettingsError> {
        Self::new(
            SettingsProvider {
                config_path: config_path.clone(),
            },
            config_path,
        )
        .await
    }
}

impl<P> ApplicationModule<P>
where
    P: Provider<Settings, Error = SettingsError> + Send + Sync,
{
    pub async fn new(settings_provider: P, config_path: PathBuf) -> Result<Self, SettingsError> {
        let settings = settings_provider.observable_cache().arc();
        let connection_pool = ConnectionPoolProvider {
            settings_cache: settings.clone(),
        }
        .observable_cache()
        .arc();
        let db_client = connection_pool
            .clone()
            .observable_cache_with_fn(DbClient::new)
            .arc();

        // Force whole provider chain, to capture errors eagerly.
        db_client.provide().await?;

        Ok(Self {
            config_path,
            settings,
            connection_pool,
            db_client,
        })
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub async fn settings(&self) -> Settings {
        self.settings.provide().await.unwrap()
    }

    /// Mutate the settings in memory, persist them to the configuration file,
    /// and invalidate the settings cache so subsequent reads observe the change.
    /// Used by the admin REST endpoints (scan directories, users, server settings).
    pub async fn update_settings<F>(&self, mutate: F) -> Result<(), SettingsError>
    where
        F: FnOnce(&mut Settings),
    {
        let mut settings = self.settings().await;
        mutate(&mut settings);
        settings.save(self.config_path())?;
        self.settings.set_expired().await;
        Ok(())
    }

    /// Invalidate the cached settings (and the caches derived from them) so the
    /// next access re-reads the configuration file. Unlike [`update_settings`],
    /// this makes no change of its own — it is the "reload config" hook for
    /// picking up edits made outside the running process.
    pub async fn reload_settings(&self) {
        self.settings.set_expired().await;
        self.connection_pool.set_expired().await;
        self.db_client.set_expired().await;
    }

    pub async fn connection_pool(&self) -> ConnectionPool {
        self.connection_pool.provide().await.unwrap()
    }

    pub async fn db_client(&self) -> DbClient {
        self.db_client.provide().await.unwrap()
    }

    pub async fn filtered_db_client(&self) -> FilteredDbClient {
        let hidden = self.settings().await.ui.hidden_tags().to_vec();
        FilteredDbClient::new(self.db_client().await, hidden)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Invalidated> {
        self.settings.subscribe()
    }

    /// Find all local files in the database whose path no longer exists on disk.
    /// If `purge` is true, also removes those stale records from the database.
    pub async fn check_missing(&self, purge: bool) -> Vec<String> {
        let connection_pool = self.connection_pool().await;
        let mut conn = connection_pool.acquire().await.expect("database available");
        let files = dao::select_all_files(&mut conn)
            .await
            .expect("database available");

        let mut missing = Vec::new();
        for file in files {
            // Archive members exist as long as their containing archive does.
            let fs_path = file.archive_path.as_ref().unwrap_or(&file.path);
            if !tokio::fs::try_exists(fs_path).await.unwrap_or(false) {
                if purge && let Err(e) = dao::delete_file_record(&connection_pool, file.id).await {
                    tracing::warn!("Failed to delete record for {}: {e}", file.path);
                }
                missing.push(file.path);
            }
        }
        missing
    }

    pub async fn extract_scan_directories(&self) {
        let settings = self.settings().await;
        let scan_directory_paths: Vec<String> = settings
            .scan
            .directories
            .iter()
            .filter(|(_, settings)| matches!(settings, DirectorySettings::Scan { .. }))
            .map(|(dir, _)| dir.display().to_string())
            .collect();

        let files: Vec<PathBuf> = self
            .db_client()
            .await
            .get_files()
            .await
            .expect("database available")
            .into_iter()
            // For archive members, the relevant on-disk location is the archive.
            .map(|f| PathBuf::from(f.archive_path.unwrap_or(f.path)))
            .collect();

        let directories: Vec<String> = files
            .iter()
            .flat_map(|f| f.parent())
            .map(|d| d.display().to_string())
            .chain(scan_directory_paths)
            .unique()
            .sorted()
            .collect();

        for dir in directories.into_iter().fold(Vec::new(), |mut acc, dir| {
            if !acc.iter().any(|d| dir.starts_with(d)) {
                acc.push(dir);
            }
            acc
        }) {
            println!("{dir}");
        }
    }
}

impl<P> Provider<Settings> for ApplicationModule<P>
where
    P: Provider<Settings, Error = SettingsError> + Send + Sync,
{
    type Error = SettingsError;

    async fn provide(&self) -> Result<Settings, Self::Error> {
        self.settings.provide().await
    }
}

impl<P> Provider<ConnectionPool> for ApplicationModule<P>
where
    P: Provider<Settings, Error = SettingsError> + Send + Sync,
{
    type Error = SettingsError;

    async fn provide(&self) -> Result<ConnectionPool, Self::Error> {
        self.connection_pool.provide().await
    }
}

impl<P> Provider<DbClient> for ApplicationModule<P>
where
    P: Provider<Settings, Error = SettingsError> + Send + Sync,
{
    type Error = SettingsError;

    async fn provide(&self) -> Result<DbClient, Self::Error> {
        self.db_client.provide().await
    }
}

impl<P> HasSetExpired for ApplicationModule<P>
where
    P: Send + Sync,
{
    async fn set_expired(&self) {
        // The order is important here, we want to expire the deepest in the chain first
        self.settings.set_expired().await;
        self.connection_pool.set_expired().await;
        self.db_client.set_expired().await;
    }
}

/// Modify `filename` by adding a number before the extension, so that it contains a filename for a not yet existing file.
/// Panics when `filename` does not end with `extension`.
/// For example:
/// - given `filename` is `my_file.txt`, `extension` is `txt` and `my_file.txt` does not exist, results in `my_file.txt`
/// - given `filename` is `my_file.txt`, `extension` is `txt` and `my_file.txt` exists, results in `my_file.1.txt`
/// - given `filename` is `my_file.txt`, `extension` is `txt` and both `my_file.txt` and `my_file.1.txt` exist, results in `my_file.2.txt`
fn to_unique_file(filename: &mut PathBuf, extension: &str) {
    // Use display as a UTF-8 string to compare
    let filename_display = filename.display().to_string();
    assert!(
        filename_display.ends_with(extension),
        "{filename_display} should end with {extension}",
    );

    let mut index: usize = 1;

    while filename.exists() {
        if index > 1 {
            filename.set_extension("");
        }
        filename.set_extension(format!("{index}.{extension}"));
        index += 1;
    }
}

/// Returns the extension of `filename`, which is everything after a `.` in the `filename`.
fn extension_of(filename: &str) -> Option<&str> {
    filename.split(".").last()
}

pub type FxIndexMap<K, V> = IndexMap<K, V, FxBuildHasher>;

/// Collect all the items from `iterator` in a [`FxIndexMap`] indexed by the key indicated by `to_key`.
/// Values are collected in a [`Vec`], so that multiple values per key are supported.
pub fn to_buckets<K, V, F>(iterator: impl Iterator<Item = V>, to_key: F) -> FxIndexMap<K, Vec<V>>
where
    K: Hash + Eq,
    F: Fn(&V) -> K,
{
    let mut buckets = FxIndexMap::default();

    for value in iterator {
        let key = to_key(&value);
        let entry = buckets.entry(key).or_insert(vec![]);
        entry.push(value)
    }

    buckets
}

pub trait Builder: Sized {
    fn apply_if<F>(self, condition: bool, fun: F) -> Self
    where
        F: FnOnce(Self) -> Self;

    fn apply_maybe<F, T>(self, option: Option<T>, fun: F) -> Self
    where
        F: FnOnce(Self, T) -> Self;

    fn apply_when<P, F>(self, predicate: P, fun: F) -> Self
    where
        P: FnOnce(&Self) -> bool,
        F: FnOnce(Self) -> Self;
}

impl<T> Builder for T {
    fn apply_if<F>(self, condition: bool, fun: F) -> Self
    where
        F: FnOnce(Self) -> Self,
    {
        if condition { fun(self) } else { self }
    }

    fn apply_maybe<F, S>(self, option: Option<S>, fun: F) -> Self
    where
        F: FnOnce(Self, S) -> Self,
    {
        if let Some(value) = option {
            fun(self, value)
        } else {
            self
        }
    }

    fn apply_when<P, F>(self, predicate: P, fun: F) -> Self
    where
        P: FnOnce(&Self) -> bool,
        F: FnOnce(Self) -> Self,
    {
        if predicate(&self) { fun(self) } else { self }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(try_from = "PathBuf", into = "PathBuf")]
pub struct ExpandedPath(PathBuf);

impl From<ExpandedPath> for PathBuf {
    fn from(value: ExpandedPath) -> Self {
        value.0
    }
}

impl ExpandedPath {
    pub fn into_inner(self) -> PathBuf {
        self.0
    }

    pub fn get_directory(&self) -> Option<PathBuf> {
        let path = self.0.canonicalize().unwrap_or_else(|_| self.0.clone());
        if path.is_dir() {
            Some(path.clone())
        } else if path.is_file() || Some(Component::RootDir) == path.components().next() {
            path.parent()
                .map(|dir| dir.into())
                .or_else(|| std::env::current_dir().ok())
        } else {
            std::env::current_dir().ok()
        }
    }

    pub fn get_full_path(&self) -> PathBuf {
        let path = self.0.canonicalize().unwrap_or_else(|_| self.0.clone());
        if !path.starts_with(std::path::MAIN_SEPARATOR_STR)
            && !path.starts_with("$")
            && !path.starts_with("%")
            && let Ok(joined_path) = std::env::current_dir().map(|dir| dir.join(&path))
        {
            joined_path
        } else {
            path
        }
    }
}

fn expand_tilde(s: &str) -> Result<PathBuf, std::io::Error> {
    if s == "~" || s.starts_with("~/") || s.starts_with("~\\") {
        let home = directories::UserDirs::new()
            .map(|u| u.home_dir().to_path_buf())
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "home directory not found")
            })?;
        Ok(home.join(&s[if s == "~" { 1 } else { 2 }..]))
    } else {
        Ok(PathBuf::from(s))
    }
}

impl FromStr for ExpandedPath {
    type Err = std::io::Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(ExpandedPath(expand_tilde(value)?))
    }
}

impl TryFrom<PathBuf> for ExpandedPath {
    type Error = std::io::Error;

    fn try_from(value: PathBuf) -> Result<Self, Self::Error> {
        Ok(ExpandedPath(expand_tilde(&value.display().to_string())?))
    }
}

impl AsRef<PathBuf> for ExpandedPath {
    fn as_ref(&self) -> &PathBuf {
        &self.0
    }
}

impl AsRef<Path> for ExpandedPath {
    fn as_ref(&self) -> &Path {
        self.0.as_path()
    }
}

impl Deref for ExpandedPath {
    type Target = PathBuf;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Display for ExpandedPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

/// Compute the SHA-256 hex digest of a file's contents.
pub async fn sha256_of_file(path: &Path) -> Result<String, io::Error> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 65536];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect())
}

pub fn force_create(path: &PathBuf) -> fs::File {
    force_create_all_parents(path);
    fs::File::create(path).unwrap()
}

pub fn force_create_all_parents(path: &Path) {
    let prefix = path.parent().unwrap();
    fs::create_dir_all(prefix).unwrap();
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;
    use rstest::rstest;

    use super::*;

    fn home() -> PathBuf {
        directories::UserDirs::new()
            .unwrap()
            .home_dir()
            .to_path_buf()
    }

    #[test]
    fn expand_tilde_alone_returns_home() {
        assert_eq!(expand_tilde("~").unwrap(), home());
    }

    #[test]
    fn expand_tilde_slash_subdir_returns_home_joined() {
        assert_eq!(expand_tilde("~/docs").unwrap(), home().join("docs"));
    }

    #[test]
    fn expand_tilde_backslash_subdir_returns_home_joined() {
        assert_eq!(expand_tilde("~\\docs").unwrap(), home().join("docs"));
    }

    #[rstest]
    #[case("/absolute/path")]
    #[case("relative/path")]
    #[case("~alice/docs")]
    #[case("notilde")]
    fn expand_tilde_non_tilde_paths_returned_as_is(#[case] input: &str) {
        assert_eq!(expand_tilde(input).unwrap(), PathBuf::from(input));
    }

    #[test]
    fn test_buckets() {
        let input = vec![0, 1, 2, 2, 3, 4, 5, 5, 6, 5, 6, 7, 8];
        let actual = to_buckets(input.into_iter(), |x| *x);
        let expected: IndexMap<i32, Vec<i32>> = vec![
            (0, vec![0]),
            (1, vec![1]),
            (2, vec![2, 2]),
            (3, vec![3]),
            (4, vec![4]),
            (5, vec![5, 5, 5]),
            (6, vec![6, 6]),
            (7, vec![7]),
            (8, vec![8]),
        ]
        .into_iter()
        .collect();
        assert_eq!(actual, expected);

        let actual: IndexMap<i32, Vec<i32>> = actual
            .into_iter()
            .filter(|(_, value)| value.len() > 1)
            .collect();
        let expected: IndexMap<i32, Vec<i32>> =
            vec![(2, vec![2, 2]), (5, vec![5, 5, 5]), (6, vec![6, 6])]
                .into_iter()
                .collect();
        assert_eq!(actual, expected);
    }
}
