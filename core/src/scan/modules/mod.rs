pub mod file_extension_finder;

use std::path::Path;

use async_trait::async_trait;

use crate::db::dao;

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
