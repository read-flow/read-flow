pub mod dao;
pub mod datasource;
pub mod models;
pub mod schema;

/// Convenience methods for acquiring a connection from the pool and running
/// one or more DAO operations, optionally inside a transaction.
pub trait ConnectionPoolExt {
    /// Obtain a connection from the pool and run `f` with it.
    fn with_connection<F, R>(&self, f: F) -> Result<R, Error>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<R, Error>;

    /// Obtain a connection from the pool and run `f` inside a database
    /// transaction.  If `f` returns `Err`, the transaction is rolled back.
    fn transaction<F, R>(&self, f: F) -> Result<R, Error>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<R, Error>;
}

impl ConnectionPoolExt for ConnectionPool {
    fn with_connection<F, R>(&self, f: F) -> Result<R, Error>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<R, Error>,
    {
        let mut conn = self.get()?;
        f(&mut conn)
    }

    fn transaction<F, R>(&self, f: F) -> Result<R, Error>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<R, Error>,
    {
        let mut conn = self.get()?;
        (*conn).transaction(f)
    }
}

use std::str::FromStr;
use std::time::Duration;

use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel::r2d2::ConnectionManager;
use diesel::r2d2::CustomizeConnection;
use diesel::r2d2::Pool;
use diesel_migrations::EmbeddedMigrations;
use diesel_migrations::MigrationHarness;
use diesel_migrations::embed_migrations;
use serde::Deserialize;
use serde::Serialize;

use crate::ExpandedPath;
use crate::db::dao::Error;

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
        Self {
            url: ExpandedPath::from_str("~/.local/share/read-flow/database.db")
                .expect("should work"),
        }
    }
}

pub type ConnectionPool = Pool<ConnectionManager<SqliteConnection>>;

#[derive(Debug)]
pub struct ConnectionOptions {
    pub enable_wal: bool,
    pub enable_foreign_keys: bool,
    pub busy_timeout: Option<Duration>,
}

/// Sensible defaults
impl Default for ConnectionOptions {
    fn default() -> Self {
        Self {
            enable_wal: true,
            enable_foreign_keys: true,
            busy_timeout: Some(Duration::from_secs(30)),
        }
    }
}

impl CustomizeConnection<SqliteConnection, diesel::r2d2::Error> for ConnectionOptions {
    fn on_acquire(&self, conn: &mut SqliteConnection) -> Result<(), diesel::r2d2::Error> {
        (|| {
            if self.enable_wal {
                conn.batch_execute("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
            }
            if self.enable_foreign_keys {
                conn.batch_execute("PRAGMA foreign_keys = ON;")?;
            }
            if let Some(d) = self.busy_timeout {
                conn.batch_execute(&format!("PRAGMA busy_timeout = {};", d.as_millis()))?;
            }
            Ok(())
        })()
        .map_err(diesel::r2d2::Error::QueryError)
    }
}

pub fn get_connection_pool(settings: &DbSettings) -> ConnectionPool {
    tracing::debug!("Creating db connection pool for: {}", &settings.url);
    tracing::debug!("Ensuring all directories exist for: {}", &settings.url);
    crate::force_create_all_parents(&settings.url);
    let manager = ConnectionManager::<SqliteConnection>::new(settings.url.to_string());

    let pool = Pool::builder()
        // .max_size(16) // SQLite only supports a single connection otherwise the logs will be cluttered with: ERROR r2d2: database is locked
        .max_size(1)
        .connection_customizer(Box::new(ConnectionOptions::default()))
        .build(manager)
        .expect("Could not build connection pool");

    run_migrations(&pool);

    pool
}

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

// TODO: error handling
fn run_migrations(connection_pool: &ConnectionPool) {
    let mut connection = connection_pool
        .get()
        .expect("Could not get connection from connection_pool");

    // This will run the necessary migrations.
    // See the documentation for `MigrationHarness` for all available methods.
    connection
        .run_pending_migrations(MIGRATIONS)
        .expect("Could not run migrations");
}
