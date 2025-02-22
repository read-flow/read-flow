pub mod dao;
pub mod datasource;
pub mod models;
pub mod schema;

use std::time::Duration;

use diesel::{
    connection::SimpleConnection,
    prelude::*,
    r2d2::{ConnectionManager, CustomizeConnection, Pool},
};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

#[derive(Debug, serde::Deserialize)]
pub struct DbSettings {
    url: String,
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
    let manager = ConnectionManager::<SqliteConnection>::new(&settings.url);

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
