pub mod api;
pub mod client;
pub mod db;
#[cfg(feature = "gui")]
pub mod gui;
pub mod scan;
#[cfg(feature = "server")]
pub mod server;
pub mod settings;
pub mod tag;

use std::{hash::Hash, path::PathBuf, sync::Arc};

use figment::Figment;
use indexmap::IndexMap;

use db::{datasource::DbClient, ConnectionPool};
use scan::FileSystemVisitor;
use settings::Settings;

#[derive(Clone, Debug)]
pub struct ApplicationModule {
    settings: Arc<Settings>,
    connection_pool: ConnectionPool,
}

impl ApplicationModule {
    pub fn instantiate() -> anyhow::Result<Self> {
        let settings = settings::extract()?;
        Ok(Self::from_settings(settings))
    }

    pub fn from_figment(figment: &Figment) -> Result<Self, figment::Error> {
        let settings = figment.extract()?;
        Ok(Self::from_settings(settings))
    }

    fn from_settings(settings: Settings) -> Self {
        let connection_pool = db::get_connection_pool(&settings.database);
        Self {
            settings: Arc::new(settings),
            connection_pool,
        }
    }

    fn db_client(&self) -> DbClient {
        DbClient::new(self.connection_pool.clone())
    }

    fn visitor(&self) -> FileSystemVisitor {
        scan::create_visitor(self.connection_pool.clone())
    }
}

/// Modify `filename` by adding a number before the extension, so that it contains a filename for a not yet existing file.
/// Panics when `filename` does not end with `extension`.
/// For example:
/// - given `filename` is `my_file.txt`, `extension` is `txt` and `my_file.txt` does not exist, results in `my_file.txt`
/// - given `filename` is `my_file.txt`, `extension` is `txt` and `my_file.txt` exists, results in `my_file.1.txt`
/// - given `filename` is `my_file.txt`, `extension` is `txt` and both `my_file.txt` and `my_file.1.txt` exist, results in `my_file.2.txt`
fn to_unique_file(filename: &mut PathBuf, extension: &str) {
    assert!(filename.ends_with(extension));

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
