use std::path::Path;

use crate::db::{ConnectionPool, dao::DirectoryDao, models::NewDirectory};

use super::{DirectoryError, DirectoryModule};

pub struct ScmProjectFinder {
    /// The hidden SCM directory, e.g. `.git`, `.hg`
    directory: String,
    connection_pool: ConnectionPool,
}

impl ScmProjectFinder {
    pub fn new(directory: String, connection_pool: ConnectionPool) -> Self {
        Self {
            directory,
            connection_pool,
        }
    }
}

impl DirectoryModule for ScmProjectFinder {
    fn matches(&self, directory: &Path) -> bool {
        directory.join(&self.directory).is_dir()
    }

    fn handle(&self, directory: &Path) -> Result<(), DirectoryError> {
        let type_ = &self.directory.to_ascii_lowercase()[1..];
        let new_directory = NewDirectory {
            path: format!("{}", directory.display()),
            type_: type_.to_owned(),
        };
        tracing::debug!("inserting directory: {}", directory.display());
        self.connection_pool.upsert_directory(new_directory)?;
        Ok(())
    }
}
