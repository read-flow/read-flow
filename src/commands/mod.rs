pub mod scan;

use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DirectoryError {}

pub trait DirectoryModule {
    fn matches(&self, _directory: &Path) -> bool {
        false
    }

    fn handle(&self, _directory: &Path) -> Result<(), DirectoryError> {
        Ok(())
    }

    fn finalize(&self) -> Result<(), DirectoryError> {
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum FileError {}

pub trait FileModule {
    fn matches(&self, _file: &Path) -> bool {
        false
    }

    fn handle(&self, _file: &Path) -> Result<(), FileError> {
        Ok(())
    }

    fn finalize(&self) -> Result<(), FileError> {
        Ok(())
    }
}
