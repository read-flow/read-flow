// SPDX-License-Identifier: AGPL-3.0-or-later

pub mod dao;
pub mod datasource;
pub mod models;

use std::str::FromStr;
use std::time::Duration;

use serde::Deserialize;
use serde::Serialize;
use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::sqlite::SqliteJournalMode;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::sqlite::SqliteSynchronous;

use crate::ExpandedPath;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct DbSettings {
    url: ExpandedPath,
}

impl DbSettings {
    pub fn url(&self) -> &ExpandedPath {
        &self.url
    }

    pub fn set_url(&mut self, url: ExpandedPath) {
        self.url = url;
    }
}

impl Default for DbSettings {
    fn default() -> Self {
        let path = directories::ProjectDirs::from("", "", "read-flow")
            .map(|d| d.data_dir().join("database.db"))
            .unwrap_or_else(|| std::path::PathBuf::from("database.db"));
        Self {
            url: ExpandedPath::from_str(&path.to_string_lossy()).expect("valid path"),
        }
    }
}

pub type ConnectionPool = SqlitePool;

pub async fn get_connection_pool(settings: &DbSettings) -> ConnectionPool {
    tracing::debug!("Creating db connection pool for: {}", &settings.url);
    tracing::debug!("Ensuring all directories exist for: {}", &settings.url);
    crate::force_create_all_parents(&settings.url);

    let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", settings.url))
        .expect("invalid database URL")
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(30))
        .create_if_missing(true)
        .pragma("cache_size", "-20000");

    let pool = SqlitePoolOptions::new()
        .max_connections(3)
        .connect_with(options)
        .await
        .expect("Could not build connection pool");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Could not run migrations");

    pool
}
