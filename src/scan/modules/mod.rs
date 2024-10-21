pub mod file_extension_finder;
pub mod scm_project_finder;

use std::path::Path;

use crate::db::dao;

#[derive(Debug, thiserror::Error)]
pub enum DirectoryError {
    #[error("error while executing database query")]
    Storage(#[from] dao::Error),
}

pub trait DirectoryModule {
    fn matches(&self, _directory: &Path) -> bool {
        false
    }

    fn handle(&self, _directory: &Path) -> Result<(), DirectoryError> {
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FileError {
    #[error("error while executing database query")]
    Storage(#[from] dao::Error),
}

pub trait FileModule {
    fn matches(&self, _file: &Path) -> bool {
        false
    }

    fn handle(&self, _file: &Path) -> Result<(), FileError> {
        Ok(())
    }
}
