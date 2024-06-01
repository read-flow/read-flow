pub mod scan;

use once_cell::sync::OnceCell;
use std::path::Path;
use thiserror::Error;

use scan::GitProjects;

pub static DIRECTORY_MODULES: OnceCell<Vec<Box<dyn DirectoryModule + Send + Sync>>> =
    OnceCell::new();

pub fn init() {
    let _directory_modules =
        DIRECTORY_MODULES.get_or_init(|| vec![Box::new(GitProjects::default())]);
}

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
