use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use sha2::Digest;
use sha2::Sha256;
use tokio::io::AsyncReadExt;

use super::FileError;
use super::FileModule;
use crate::api::ReadingStatus;
use crate::db::ConnectionPool;
use crate::db::dao;
use crate::db::models::FileTag;
use crate::db::models::NewFile;
use crate::scan::DirectorySettings;
use crate::scan::ScanSettings;

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

#[async_trait]
impl FileModule for FileExtensionFinder {
    fn matches(&self, file: &Path) -> bool {
        file.extension()
            .eq(&Some(std::ffi::OsStr::new(&self.extension)))
    }

    async fn handle(&self, file: &Path) -> Result<(), FileError> {
        match self
            .scan_settings
            .directory_settings_of(file)
            .unwrap_or(DirectorySettings::empty_scan())
        {
            DirectorySettings::Ignore { .. } => Ok(()),
            DirectorySettings::Scan { tags, .. } => {
                let extension = self.extension.to_ascii_lowercase();

                if self.scan_settings.dry_run {
                    tracing::debug!("[dry_run] found file: {}", file.display());
                } else {
                    tracing::debug!("inserting file: {}", file.display());
                    let new_file = to_new_file(file, &extension).await;

                    dao::upsert_file(&self.connection_pool, new_file).await?;

                    // unwrap is safe: the file was just inserted
                    let db_file = dao::select_file_by_path(
                        &self.connection_pool,
                        &file.display().to_string(),
                    )
                    .await?
                    .unwrap();

                    tracing::debug!("inserting tags: {tags:?} for file: {}", db_file.path);

                    let file_tags: Vec<_> = tags
                        .into_iter()
                        .map(|tag| FileTag::new(db_file.id, tag))
                        .collect();

                    dao::upsert_many_file_tags(&self.connection_pool, file_tags).await?;
                }

                Ok(())
            }
        }
    }
}

async fn to_new_file(file: &Path, extension: &str) -> NewFile {
    let metadata = tokio::fs::metadata(file)
        .await
        .expect("failed to get file metadata");
    let size: i32 = metadata
        .len()
        .try_into()
        .expect("failed to convert file size to i32");
    let fingerprint = compute_sha256(file)
        .await
        .expect("failed to calculate the fingerprint");
    NewFile {
        path: file.display().to_string(),
        type_: extension.to_owned(),
        size,
        fingerprint,
        status: ReadingStatus::Unread.into(),
    }
}

async fn compute_sha256(path: &Path) -> Result<String, std::io::Error> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
