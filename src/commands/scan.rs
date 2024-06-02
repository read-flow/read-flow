use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use super::{DirectoryError, DirectoryModule, FileError, FileModule};

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

pub struct FileExtensionFinder {
    extension: String,
    files: Mutex<Vec<PathBuf>>,
}

impl FileExtensionFinder {
    pub fn new(extension: String) -> Self {
        Self {
            extension,
            files: vec![].into(),
        }
    }
}

impl FileModule for FileExtensionFinder {
    fn matches(&self, file: &Path) -> bool {
        file.extension()
            .eq(&Some(std::ffi::OsStr::new(&self.extension)))
    }

    fn handle(&self, file: &Path) -> Result<(), FileError> {
        let mut files = self.files.lock().unwrap();
        files.push(file.to_owned());
        Ok(())
    }

    fn finalize(&self) -> Result<(), FileError> {
        let files = self.files.lock().unwrap();
        let extension = self.extension.to_ascii_uppercase();

        println!("{extension} files found: {files:?}");
        Ok(())
    }
}
