use std::{
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
};

use diesel::prelude::*;
use rayon::prelude::*;

use crate::db::{models::NewFile, schema::files, ConnectionPool};

use super::{FileError, FileModule};

pub struct FileExtensionFinder {
    extension: String,
    files: Mutex<Vec<PathBuf>>,
    connection_pool: ConnectionPool,
}

impl FileExtensionFinder {
    pub fn new(extension: String, connection_pool: ConnectionPool) -> Self {
        Self {
            extension,
            files: vec![].into(),
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
        let mut files = self.files.lock().unwrap();
        files.push(file.to_owned());
        Ok(())
    }

    fn finalize(&self) -> Result<(), FileError> {
        let path_bufs = self.files.lock().unwrap();
        let extension = self.extension.to_ascii_uppercase();
        tracing::debug!("{extension} files found: {path_bufs:?}");

        let entities: Vec<_> = path_bufs
            .par_iter()
            .map(|file| {
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
                    type_: extension.clone(),
                    size,
                    sha256sum: sha256sum.to_string(),
                }
            })
            .collect();

        let mut connection = self.connection_pool.get()?;

        diesel::insert_into(files::table)
            .values(entities)
            .execute(&mut connection)?;

        tracing::debug!("files added to the database");

        Ok(())
    }
}
