pub mod api;
pub mod client;
pub mod db;
pub mod scan;
#[cfg(feature = "server")]
pub mod server;
pub mod settings;
pub mod tag;

use std::hash::Hash;
use std::ops::Deref;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use api::FileDataSource;
use db::ConnectionPool;
use db::datasource::DbClient;
use expanduser::expanduser;
use figment::Figment;
use indexmap::IndexMap;
use itertools::Itertools;
use scan::DirectorySettings;
use scan::FileSystemVisitor;
use serde::Deserialize;
use serde::Serialize;
use settings::Settings;
use settings::SettingsError;
use tokio::runtime::Runtime;

#[derive(Clone, Debug)]
pub struct ApplicationModule {
    pub settings: Arc<Settings>,
    pub connection_pool: ConnectionPool,
}

impl ApplicationModule {
    pub fn instantiate() -> anyhow::Result<Self> {
        let settings = settings::extract()?;
        Ok(Self::from_settings(settings))
    }

    pub fn from_figment(figment: &Figment) -> Result<Self, SettingsError> {
        let settings = figment.extract()?;
        Ok(Self::from_settings(settings))
    }

    pub fn from_settings(settings: Settings) -> Self {
        let connection_pool = db::get_connection_pool(&settings.database);

        Self {
            settings: Arc::new(settings),
            connection_pool,
        }
    }

    pub fn db_client(&self) -> DbClient {
        DbClient::new(self.connection_pool.clone())
    }

    fn visitor(&self) -> FileSystemVisitor {
        scan::create_visitor(self.connection_pool.clone(), self.settings.scan.clone())
    }

    pub fn extract_scan_directories(&self) {
        // Create the runtime
        let rt = Runtime::new().unwrap();

        let scan_directories = self
            .settings
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
                .map(|d| format!("{}", d.display()))
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

/// Modify `filename` by adding a number before the extension, so that it contains a filename for a not yet existing file.
/// Panics when `filename` does not end with `extension`.
/// For example:
/// - given `filename` is `my_file.txt`, `extension` is `txt` and `my_file.txt` does not exist, results in `my_file.txt`
/// - given `filename` is `my_file.txt`, `extension` is `txt` and `my_file.txt` exists, results in `my_file.1.txt`
/// - given `filename` is `my_file.txt`, `extension` is `txt` and both `my_file.txt` and `my_file.1.txt` exist, results in `my_file.2.txt`
fn to_unique_file(filename: &mut PathBuf, extension: &str) {
    // Use display as a UTF-8 string to compare
    let filename_display = format!("{}", filename.display());
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
}

impl<T> Builder for T {
    fn apply_if<F>(self, condition: bool, fun: F) -> Self
    where
        F: FnOnce(Self) -> Self,
    {
        if condition { fun(self) } else { self }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(try_from = "PathBuf", into = "PathBuf")]
pub struct ExpandedPath(PathBuf);

impl From<ExpandedPath> for PathBuf {
    fn from(value: ExpandedPath) -> Self {
        value.0
    }
}

// impl ExpandedPath {
//     fn into_inner(self) -> PathBuf {
//         self.0
//     }
// }

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
        let expanded = expanduser(format!("{}", value.display()))?;
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
