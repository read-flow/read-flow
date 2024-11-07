pub mod file_system_visitor;
pub mod modules;

use std::path::PathBuf;

use anyhow::Result;
use indexmap::IndexMap;
use serde::Deserialize;

use crate::{db::ConnectionPool, ApplicationModule};

pub use file_system_visitor::{Error, FileSystemVisitor};
use modules::{file_extension_finder::FileExtensionFinder, scm_project_finder::ScmProjectFinder};

#[derive(Debug, Deserialize)]
pub struct ScanSettings {
    pub dry_run: bool,
    pub auto_tags: IndexMap<String, Vec<String>>,
}

pub fn create_visitor(connection_pool: ConnectionPool) -> FileSystemVisitor {
    FileSystemVisitor::new(
        vec![
            Box::new(ScmProjectFinder::new(
                ".git".into(),
                connection_pool.clone(),
            )),
            Box::new(ScmProjectFinder::new(".hg".into(), connection_pool.clone())),
        ],
        vec![
            Box::new(FileExtensionFinder::new(
                "pdf".into(),
                connection_pool.clone(),
            )),
            Box::new(FileExtensionFinder::new(
                "epub".into(),
                connection_pool.clone(),
            )),
            Box::new(FileExtensionFinder::new("mobi".into(), connection_pool)),
        ],
    )
}

impl ApplicationModule {
    pub fn scan(self, path: PathBuf) -> Result<()> {
        let path = path.canonicalize()?;
        self.visitor().visit(&path)?;
        Ok(())
    }
}
