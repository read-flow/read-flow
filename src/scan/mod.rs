pub mod file_system_visitor;
pub mod modules;

use std::path::PathBuf;

use anyhow::Result;

use crate::db::ConnectionPool;

pub use file_system_visitor::{Error, FileSystemVisitor};
use modules::{file_extension_finder::FileExtensionFinder, scm_project_finder::ScmProjectFinder};

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

pub fn scan(path: PathBuf, connection_pool: ConnectionPool) -> Result<()> {
    let visitor = create_visitor(connection_pool);
    let path = path.canonicalize()?;
    visitor.visit(&path)?;

    Ok(())
}
