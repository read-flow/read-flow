pub mod api;
pub mod client;
pub mod db;
pub mod scan;
#[cfg(feature = "server")]
pub mod server;
pub mod settings;
pub mod tag;

use std::fmt;
use std::hash::Hash;
use std::ops::Deref;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use api::FileDataSource;
use db::ConnectionPool;
use db::datasource::DbClient;
use expanduser::expanduser;
use indexmap::IndexMap;
use itertools::Itertools;
use provider::sync::HasSetExpired;
use provider::sync::Invalidated;
use provider::sync::Observable;
use provider::sync::ObservableCache;
use provider::sync::Provider;
use scan::DirectorySettings;
use scan::FileSystemVisitor;
use serde::Deserialize;
use serde::Serialize;
use settings::Settings;
use settings::SettingsError;
use tokio::runtime::Runtime;
use tokio::sync::broadcast;

type SettingsCache<P> = ObservableCache<P, fn(Settings) -> Settings, Settings, Settings>;
type ConnectionPoolCache<P> = ObservableCache<
    Arc<SettingsCache<P>>,
    fn(Settings) -> ConnectionPool,
    Settings,
    ConnectionPool,
>;
type ClientCache<P> = ObservableCache<
    Arc<ConnectionPoolCache<P>>,
    fn(ConnectionPool) -> DbClient,
    ConnectionPool,
    DbClient,
>;

#[derive(Debug)]
pub struct ApplicationModule<P> {
    config_path: PathBuf,
    settings: Arc<SettingsCache<P>>,
    connection_pool: Arc<ConnectionPoolCache<P>>,
    db_client: Arc<ClientCache<P>>,
}

pub struct SettingsProvider;

impl Provider<Settings> for SettingsProvider {
    type Error = SettingsError;
    fn provide(&self) -> Result<Settings, Self::Error> {
        settings::extract()
    }
}

pub struct ScanSettingsProvider {
    pub dry_run: bool,
}

impl Provider<Settings> for ScanSettingsProvider {
    type Error = SettingsError;
    fn provide(&self) -> Result<Settings, Self::Error> {
        let mut settings = settings::extract()?;
        settings.scan.merge_dry_run(self.dry_run);
        Ok(settings)
    }
}

impl ApplicationModule<SettingsProvider> {
    pub fn instantiate() -> Result<Self, SettingsError> {
        Self::new(SettingsProvider, settings::config_path())
    }
}

impl<P> ApplicationModule<P>
where
    P: Provider<Settings, Error = SettingsError>,
{
    pub fn new(settings_provider: P, config_path: PathBuf) -> Result<Self, SettingsError> {
        let settings = settings_provider.observable_cache().arc();
        let connection_pool = settings
            .clone()
            .observable_cache_with_fn(|settings: Settings| {
                db::get_connection_pool(&settings.database)
            })
            .arc();
        let db_client = connection_pool
            .clone()
            .observable_cache_with_fn(DbClient::new)
            .arc();

        // Force whole provider chain, to capture errors eagerly.
        db_client.provide()?;

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

    pub fn settings(&self) -> Settings {
        self.settings.provide().unwrap()
    }

    pub fn connection_pool(&self) -> ConnectionPool {
        self.connection_pool.provide().unwrap()
    }

    pub fn db_client(&self) -> DbClient {
        self.db_client.provide().unwrap()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Invalidated> {
        self.settings.subscribe()
    }

    fn visitor(&self) -> FileSystemVisitor {
        scan::create_visitor(self.connection_pool(), self.settings().scan)
    }

    pub fn extract_scan_directories(&self) {
        // Create the runtime
        let rt = Runtime::new().unwrap();

        let settings = self.settings();
        let scan_directories = settings
            .scan
            .directories
            .iter()
            .filter(|(_, settings)| matches!(settings, DirectorySettings::Scan { .. }))
            .map(|(dir, _)| dir.as_ref())
            .collect::<Vec<_>>();

        // Execute the future, blocking the current thread until completion
        rt.block_on(async {
            let files: Vec<PathBuf> = self
                .db_client()
                .get_files()
                .await
                .expect("database available")
                .into_iter()
                .map(|f| PathBuf::from(f.path))
                .collect();

            let directories: Vec<String> = files
                .iter()
                .flat_map(|f| f.parent())
                .chain(scan_directories)
                .map(|d| d.display().to_string())
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
        });
    }
}

impl<P> Provider<Settings> for ApplicationModule<P>
where
    P: Provider<Settings, Error = SettingsError>,
{
    type Error = SettingsError;

    fn provide(&self) -> Result<Settings, Self::Error> {
        self.settings.provide()
    }
}

impl<P> Provider<ConnectionPool> for ApplicationModule<P>
where
    P: Provider<Settings, Error = SettingsError>,
{
    type Error = SettingsError;

    fn provide(&self) -> Result<ConnectionPool, Self::Error> {
        self.connection_pool.provide()
    }
}

impl<P> Provider<DbClient> for ApplicationModule<P>
where
    P: Provider<Settings, Error = SettingsError>,
{
    type Error = SettingsError;

    fn provide(&self) -> Result<DbClient, Self::Error> {
        self.db_client.provide()
    }
}

impl<P> HasSetExpired for ApplicationModule<P> {
    fn set_expired(&self) {
        // The order is important here, we want to expire the deepest in the chain first
        self.settings.set_expired();
        self.connection_pool.set_expired();
        self.db_client.set_expired();
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

/// Collect all the items from `iterator` in a [`IndexMap`] indexed by the key indicated by `to_key`.
/// Values are collected in a [`Vec`], so that multiple values per key are supported.
pub fn to_buckets<K, V, F>(iterator: impl Iterator<Item = V>, to_key: F) -> IndexMap<K, Vec<V>>
where
    K: Hash + Eq,
    F: Fn(&V) -> K,
{
    let mut buckets = IndexMap::default();

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

impl FromStr for ExpandedPath {
    type Err = std::io::Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let expanded = expanduser(value)?;
        Ok(ExpandedPath(expanded))
    }
}

impl TryFrom<PathBuf> for ExpandedPath {
    type Error = std::io::Error;

    fn try_from(value: PathBuf) -> Result<Self, Self::Error> {
        let expanded = expanduser(value.display().to_string())?;
        Ok(ExpandedPath(expanded))
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
        write!(f, "{}", &self.0.display())
    }
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;

    use super::*;

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
