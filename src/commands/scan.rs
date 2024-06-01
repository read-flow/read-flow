use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use thiserror::Error;

use super::{
    DirectoryError, DirectoryModule, FileError, FileModule, DIRECTORY_MODULES, FILE_MODULES,
};

#[derive(Debug, Error)]
enum ScanError {
    #[error("Supplied path `{0}` is not a directory")]
    NotADirectory(PathBuf),
}

fn is_not_hidden(file: &Path) -> bool {
    file.file_name()
        .and_then(|f| f.to_str())
        .map(|f| !f.starts_with('.'))
        .unwrap_or(false)
}

pub fn scan(directory: &Path) -> anyhow::Result<()> {
    dbg!(&directory);

    if !directory.is_dir() {
        Err(ScanError::NotADirectory(directory.to_path_buf()))?;
    }

    let directory_modules = DIRECTORY_MODULES
        .get()
        .expect("Should be initialized by now");

    let directory_module = directory_modules.iter().find(|m| m.matches(directory));

    match directory_module {
        Some(directory_module) => {
            directory_module.handle(directory)?;
        }
        None => {
            let (directories, files): (Vec<PathBuf>, Vec<PathBuf>) = std::fs::read_dir(directory)?
                .map(|f| f.unwrap().path())
                .partition(|f| f.is_dir());

            for dir in directories {
                if is_not_hidden(&dir) {
                    scan(&dir)?;
                }
            }

            for file in files {
                scan_file(&file)?;
            }
        }
    }

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

pub fn scan_file(file: &Path) -> anyhow::Result<()> {
    dbg!(&file);

    let file_modules = FILE_MODULES.get().expect("Should be initialized by now");

    let file_module = file_modules.iter().find(|m| m.matches(file));

    if let Some(file_module) = file_module {
        file_module.handle(file)?;
    }

    Ok(())
}

pub struct PdfFiles {
    files: Mutex<Vec<PathBuf>>,
}

impl Default for PdfFiles {
    fn default() -> Self {
        Self {
            files: vec![].into(),
        }
    }
}

impl FileModule for PdfFiles {
    fn matches(&self, file: &Path) -> bool {
        file.extension().eq(&Some(std::ffi::OsStr::new("pdf")))
    }

    fn handle(&self, file: &Path) -> Result<(), FileError> {
        let mut files = self.files.lock().unwrap();
        files.push(file.to_owned());
        Ok(())
    }

    fn finalize(&self) -> Result<(), FileError> {
        let files = self.files.lock().unwrap();
        println!("PDF files found: {files:?}");
        Ok(())
    }
}
