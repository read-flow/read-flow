pub mod scan;

use once_cell::sync::OnceCell;
use std::path::Path;
use thiserror::Error;

use scan::{GitProjects, PdfFiles};

pub static DIRECTORY_MODULES: OnceCell<Vec<Box<dyn DirectoryModule + Send + Sync>>> =
    OnceCell::new();

pub fn init() {
    DIRECTORY_MODULES.get_or_init(|| vec![Box::<GitProjects>::default()]);
    FILE_MODULES.get_or_init(|| vec![Box::<PdfFiles>::default()]);
}

pub fn finalize() {
    DIRECTORY_MODULES
        .get()
        .expect("Should be initialized by now")
        .iter()
        .for_each(|m| m.finalize().unwrap());

    FILE_MODULES
        .get()
        .expect("Should be initialized by now")
        .iter()
        .for_each(|m| m.finalize().unwrap());
}

pub static FILE_MODULES: OnceCell<Vec<Box<dyn FileModule + Send + Sync>>> = OnceCell::new();

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
