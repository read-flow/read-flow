use std::{os::unix::fs::MetadataExt, path::Path, process::Command};

use crate::db::{dao::FileDao, models::NewFile, ConnectionPool};

use super::{FileError, FileModule};

pub struct FileExtensionFinder {
    extension: String,
    connection_pool: ConnectionPool,
}

impl FileExtensionFinder {
    pub fn new(extension: String, connection_pool: ConnectionPool) -> Self {
        Self {
            extension,
            connection_pool,
        }
    }
}

impl FileModule for FileExtensionFinder {
    fn matches(&self, file: &Path) -> bool {
        file.extension()
            .eq(&Some(std::ffi::OsStr::new(&self.extension)))
    }

    fn handle(&self, file: &Path) -> Result<(), FileError> {
        let extension = self.extension.to_ascii_lowercase();
        let new_file = to_new_file(file, &extension);
        tracing::debug!("inserting file: {}", file.display());
        self.connection_pool.upsert_file(new_file)?;
        Ok(())
    }
}

pub fn to_new_file(file: &Path, extension: &str) -> NewFile {
    let size = file
        .metadata()
        .expect("failed to get file metadata")
        .size()
        .try_into()
        .expect("failed to convert file size to i32");
    let output = Command::new("sha256sum")
        .arg(file)
        .output()
        .expect("failed to calculate the sha256sum");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let sha256sum = stdout.split(' ').next().expect("expected sha256sum");
    NewFile {
        path: format!("{}", file.display()),
        type_: extension.to_owned(),
        size,
        sha256sum: sha256sum.to_string(),
    }
}
