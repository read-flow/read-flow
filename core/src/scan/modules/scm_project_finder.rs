use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;

use super::DirectoryError;
use super::DirectoryModule;
use crate::db::ConnectionPool;
use crate::db::dao;
use crate::db::models::NewDirectory;
use crate::scan::ScanSettings;

pub struct ScmProjectFinder {
    /// The hidden SCM directory, e.g. `.git`, `.hg`
    directory: String,
    connection_pool: ConnectionPool,
    scan_settings: Arc<ScanSettings>,
}

impl ScmProjectFinder {
    pub fn new(
        directory: String,
        connection_pool: ConnectionPool,
        scan_settings: Arc<ScanSettings>,
    ) -> Self {
        Self {
            directory,
            connection_pool,
            scan_settings,
        }
    }
}

#[async_trait]
impl DirectoryModule for ScmProjectFinder {
    fn matches(&self, directory: &Path) -> bool {
        directory.join(&self.directory).is_dir()
    }

    async fn handle(&self, directory: &Path) -> Result<(), DirectoryError> {
        let type_ = &self.directory.to_ascii_lowercase()[1..];
        let new_directory = NewDirectory {
            path: directory.display().to_string(),
            type_: type_.to_owned(),
        };

        if self.scan_settings.dry_run {
            tracing::debug!("[dry_run] found directory: {}", directory.display());
        } else {
            tracing::debug!("inserting directory: {}", directory.display());
            dao::upsert_directory(&self.connection_pool, new_directory).await?;
        }

        Ok(())
    }
}
