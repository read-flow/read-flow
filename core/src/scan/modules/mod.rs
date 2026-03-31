pub mod file_extension_finder;
pub mod scm_project_finder;

use std::path::Path;

use async_trait::async_trait;

use crate::db::dao;

#[derive(Debug, thiserror::Error)]
pub enum DirectoryError {
    #[error("error while executing database query")]
    Storage(#[from] dao::Error),
}

#[async_trait]
pub trait DirectoryModule: Send + Sync {
    fn matches(&self, _directory: &Path) -> bool {
        false
    }

    async fn handle(&self, _directory: &Path) -> Result<(), DirectoryError> {
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FileError {
    #[error("error while executing database query")]
    Storage(#[from] dao::Error),
}

#[async_trait]
pub trait FileModule: Send + Sync {
    fn matches(&self, _file: &Path) -> bool {
        false
    }

    async fn handle(&self, _file: &Path) -> Result<(), FileError> {
        Ok(())
    }
}
