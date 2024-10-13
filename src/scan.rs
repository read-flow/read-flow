use std::path::PathBuf;

use anyhow::Result;

use crate::{
    file_system_visitor::FileSystemVisitor,
    get_connection_pool,
    modules::{file_extension_finder::FileExtensionFinder, scm_project_finder::ScmProjectFinder},
};

pub fn scan(path: PathBuf) -> Result<()> {
    let connection_pool = get_connection_pool();

    let visitor = FileSystemVisitor::new(
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
    );

    let path = path.canonicalize()?;
    visitor.visit(&path)?;

    visitor.finalize();

    Ok(())
}
