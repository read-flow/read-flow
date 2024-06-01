use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use thiserror::Error;

use super::{DirectoryError, DirectoryModule, DIRECTORY_MODULES};

#[derive(Debug, Error)]
enum ScanError {
    #[error("Supplied path `{0}` is not a directory")]
    NotADirectory(PathBuf),
}

pub fn scan(directory: &Path) -> anyhow::Result<()> {
    if !directory.is_dir() {
        Err(ScanError::NotADirectory(directory.to_path_buf()))?;
    }

    let directory_modules = DIRECTORY_MODULES
        .get()
        .expect("Should be initialized by now");

    directory_modules
        .iter()
        .filter(|m| m.matches(directory))
        .map(|m| m.handle(directory))
        .collect::<Result<Vec<()>, _>>()?;

    directory_modules.iter().for_each(|m| m.finalize().unwrap());

    Ok(())
}

pub struct GitProjects {
    projects: Mutex<Vec<PathBuf>>,
}

impl Default for GitProjects {
    fn default() -> Self {
        Self {
            projects: vec![].into(),
        }
    }
}

impl DirectoryModule for GitProjects {
    fn matches(&self, directory: &Path) -> bool {
        directory.join(".git").is_dir()
    }

    fn handle(&self, directory: &Path) -> Result<(), DirectoryError> {
        let mut projects = self.projects.lock().unwrap();
        projects.push(directory.to_owned());
        Ok(())
    }

    fn finalize(&self) -> Result<(), DirectoryError> {
        let projects = self.projects.lock().unwrap();
        println!("Git projects found: {projects:?}");
        Ok(())
    }
}

pub fn scan_file(_file: &Path) -> anyhow::Result<()> {
    Ok(())
}
