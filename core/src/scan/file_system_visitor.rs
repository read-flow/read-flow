use std::io;
use std::path::Path;

use rayon::prelude::*;

use super::modules::DirectoryError;
use super::modules::DirectoryModule;
use super::modules::FileError;
use super::modules::FileModule;

fn is_not_hidden(file: &Path) -> bool {
    file.file_name()
        .and_then(|f| f.to_str())
        .map(|f| !f.starts_with('.'))
        .unwrap_or(false)
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("file module error: {0}")]
    FileModule(#[from] FileError),
    #[error("directory module error: {0}")]
    Directory(#[from] DirectoryError),
    #[error("file system error: {0}")]
    IO(#[from] io::Error),
}

type Result<T> = std::result::Result<T, Error>;

pub struct FileSystemVisitor {
    directory_modules: Vec<Box<dyn DirectoryModule + Send + Sync>>,
    file_modules: Vec<Box<dyn FileModule + Send + Sync>>,
}

impl FileSystemVisitor {
    pub fn new(
        directory_modules: Vec<Box<dyn DirectoryModule + Send + Sync>>,
        file_modules: Vec<Box<dyn FileModule + Send + Sync>>,
    ) -> Self {
        Self {
            directory_modules,
            file_modules,
        }
    }

    pub fn visit(&self, path: &Path) -> Result<()> {
        if path.is_dir() {
            self.visit_directory(path)
        } else {
            self.visit_file(path)
        }
    }

    fn visit_directory(&self, directory: &Path) -> Result<()> {
        tracing::debug!("visiting directory: {directory:?}");

        let directory_module = self.directory_modules.iter().find(|m| m.matches(directory));

        match directory_module {
            Some(directory_module) => {
                directory_module.handle(directory)?;
            }
            None => {
                std::fs::read_dir(directory)?
                    .par_bridge()
                    .map(|f| f.unwrap().path())
                    .filter(|path| is_not_hidden(path))
                    .for_each(|path| {
                        if let Err(error) = self.visit(&path) {
                            tracing::error!("error while visiting {path:?}: {error:?}");
                        }
                    });
            }
        }

        Ok(())
    }

    fn find_module_for_file(&self, file: &Path) -> Option<&(dyn FileModule + Send + Sync)> {
        self.file_modules
            .iter()
            .find(|m| m.matches(file))
            .map(|m| &**m)
    }

    fn visit_file(&self, file: &Path) -> Result<()> {
        tracing::debug!("visiting file: {file:?}");

        let file_module = self.find_module_for_file(file);

        if let Some(file_module) = file_module {
            file_module.handle(file)?;
        }

        Ok(())
    }
}
