use std::path::Path;

use crate::modules::{FileModule, DirectoryModule};

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
        self.file_modules.iter().for_each(|m| m.finalize().unwrap());

        self.directory_modules
            .iter()
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
        dbg!(&directory);

        let directory_module = self.directory_modules.iter().find(|m| m.matches(directory));

        match directory_module {
            Some(directory_module) => {
                directory_module.handle(directory)?;
            }
            None => {
                for path in std::fs::read_dir(directory)?.map(|f| f.unwrap().path()) {
                    if is_not_hidden(&path) {
                        self.visit(&path)?
                    }
                }
            }
        }

        Ok(())
    }

    fn visit_file(&self, file: &Path) -> anyhow::Result<()> {
        dbg!(&file);

        let file_module = self.file_modules.iter().find(|m| m.matches(file));

        if let Some(file_module) = file_module {
            file_module.handle(file)?;
        }

        Ok(())
    }
}
