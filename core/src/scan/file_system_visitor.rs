use std::future::Future;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use super::modules::FileError;
use super::modules::FileModule;

const SCM_ROOTS: &[&str] = &[".git", ".hg"];

fn is_not_hidden(file: &Path) -> bool {
    file.file_name()
        .and_then(|f| f.to_str())
        .map(|f| !f.starts_with('.'))
        .unwrap_or(false)
}

fn is_scm_root(directory: &Path) -> bool {
    SCM_ROOTS
        .iter()
        .any(|marker| directory.join(marker).is_dir())
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("file module error: {0}")]
    FileModule(#[from] FileError),
    #[error("file system error: {0}")]
    IO(#[from] io::Error),
}

type Result<T> = std::result::Result<T, Error>;

pub struct FileSystemVisitor {
    file_modules: Vec<Box<dyn FileModule>>,
}

impl FileSystemVisitor {
    pub fn new(file_modules: Vec<Box<dyn FileModule>>) -> Self {
        Self { file_modules }
    }

    pub fn visit(
        self: Arc<Self>,
        path: PathBuf,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        Box::pin(async move {
            if tokio::fs::metadata(&path).await?.is_dir() {
                self.visit_directory(path).await
            } else {
                self.visit_file(path).await
            }
        })
    }

    async fn visit_directory(self: Arc<Self>, directory: PathBuf) -> Result<()> {
        tracing::debug!("visiting directory: {directory:?}");

        if is_scm_root(&directory) {
            tracing::debug!("skipping SCM root: {directory:?}");
            return Ok(());
        }

        let mut read_dir = tokio::fs::read_dir(&directory).await?;
        let mut paths: Vec<PathBuf> = Vec::new();
        while let Some(entry) = read_dir.next_entry().await? {
            let path = entry.path();
            if is_not_hidden(&path) {
                paths.push(path);
            }
        }

        for path in paths {
            if let Err(error) = Arc::clone(&self).visit(path).await {
                tracing::error!("error while visiting: {error:?}");
            }
        }

        Ok(())
    }

    async fn visit_file(self: Arc<Self>, file: PathBuf) -> Result<()> {
        tracing::debug!("visiting file: {file:?}");

        let module_idx = self.file_modules.iter().position(|m| m.matches(&file));

        if let Some(idx) = module_idx {
            self.file_modules[idx].handle(&file).await?;
        }

        Ok(())
    }
}
