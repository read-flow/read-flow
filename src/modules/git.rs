use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use super::{DirectoryError, DirectoryModule};

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
