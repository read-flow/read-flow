use std::path::Path;

use rayon::prelude::*;

use crate::modules::{DirectoryModule, FileModule};

fn is_not_hidden(file: &Path) -> bool {
    file.file_name()
        .and_then(|f| f.to_str())
        .map(|f| !f.starts_with('.'))
        .unwrap_or(false)
}

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

    pub fn finalize(self) {
        self.file_modules
            .par_iter()
            .for_each(|m| m.finalize().unwrap());

        self.directory_modules
            .par_iter()
            .for_each(|m| m.finalize().unwrap());
    }

    pub fn visit(&self, path: &Path) -> anyhow::Result<()> {
        if path.is_dir() {
            self.visit_directory(path)
        } else {
            self.visit_file(path)
        }
    }

    fn visit_directory(&self, directory: &Path) -> anyhow::Result<()> {
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

    fn visit_file(&self, file: &Path) -> anyhow::Result<()> {
        tracing::debug!("visiting file: {file:?}");

        let file_module = self.file_modules.iter().find(|m| m.matches(file));

        if let Some(file_module) = file_module {
            file_module.handle(file)?;
        }

        Ok(())
    }
}
