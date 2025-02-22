use std::{os::unix::fs::MetadataExt, path::Path, process::Command, sync::Arc};

use crate::{
    db::{
        ConnectionPool,
        dao::{FileDao, FileTagDao},
        models::{FileTag, NewFile},
    },
    scan::{DirectorySettings, ScanSettings},
};

use super::{FileError, FileModule};

pub struct FileExtensionFinder {
    extension: String,
    connection_pool: ConnectionPool,
    scan_settings: Arc<ScanSettings>,
}

impl FileExtensionFinder {
    pub fn new(
        extension: String,
        connection_pool: ConnectionPool,
        scan_settings: Arc<ScanSettings>,
    ) -> Self {
        Self {
            extension,
            connection_pool,
            scan_settings,
        }
    }
}

impl FileModule for FileExtensionFinder {
    fn matches(&self, file: &Path) -> bool {
        file.extension()
            .eq(&Some(std::ffi::OsStr::new(&self.extension)))
    }

    fn handle(&self, file: &Path) -> Result<(), FileError> {
        match self
            .scan_settings
            .directory_settings_of(file)
            .unwrap_or(DirectorySettings::empty_scan())
        {
            DirectorySettings::Ignore { .. } => Ok(()),
            DirectorySettings::Scan { tags, .. } => {
                let extension = self.extension.to_ascii_lowercase();
                let new_file = to_new_file(file, &extension);
                tracing::debug!("inserting file: {}", file.display());
                self.connection_pool.upsert_file(new_file)?;

                // unwrap is safe, because the file is just added
                let db_file = self
                    .connection_pool
                    .select_file_by_path(&format!("{}", file.display()))?
                    .unwrap();

                tracing::debug!("inserting tags: {tags:?} for file: {}", db_file.path);

                let file_tags: Vec<_> = tags
                    .into_iter()
                    .map(|tag| FileTag::new(db_file.id, tag))
                    .collect();

                self.connection_pool.upsert_many_file_tags(file_tags)?;

                Ok(())
            }
        }
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
        .expect("failed to calculate the fingerprint");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let fingerprint = stdout.split(' ').next().expect("expected fingerprint");
    NewFile {
        path: format!("{}", file.display()),
        type_: extension.to_owned(),
        size,
        fingerprint: fingerprint.to_string(),
    }
}
