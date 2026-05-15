use std::collections::HashMap;
use std::path::Path;
use std::process::ExitStatus;
use std::sync::Arc;

use sha2::Digest;
use sha2::Sha256;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use super::ConnectionPool;
use super::dao;
use super::dao::Error;
use crate::api::File;
use crate::api::FileDataSource;
use crate::api::ReadingProgress;
use crate::api::Status;
use crate::db::models::ContentMetadata;
use crate::db::models::ContentTag;
use crate::db::models::NewFile;

#[derive(Clone)]
pub struct DbClient {
    connection_pool: ConnectionPool,
}

impl DbClient {
    pub fn new(connection_pool: ConnectionPool) -> Self {
        Self { connection_pool }
    }
}

#[async_trait::async_trait]
impl FileDataSource for DbClient {
    type Error = Error;

    fn display_name(&self) -> String {
        "Local Files".to_string()
    }

    async fn status(&self) -> Result<Status, Self::Error> {
        let mut conn = self.connection_pool.acquire().await?;
        sqlx::query("SELECT 1").execute(&mut *conn).await?;
        Ok(Status {
            identifier: "database".to_string(),
            ..Default::default()
        })
    }

    async fn get_files(&self) -> Result<Vec<File>, Self::Error> {
        let mut conn = self.connection_pool.acquire().await?;
        let files = dao::select_all_files(&mut conn).await?;
        let all_tags = dao::select_all_content_tags(&mut conn).await?;

        let mut tags_by_fp: HashMap<String, Vec<ContentTag>> = HashMap::new();
        for tag in all_tags {
            tags_by_fp
                .entry(tag.fingerprint.clone())
                .or_default()
                .push(tag);
        }

        Ok(files
            .into_iter()
            .map(|file| {
                let tags = tags_by_fp.remove(&file.fingerprint).unwrap_or_default();
                (file, tags).into()
            })
            .collect())
    }

    async fn get_files_tags(&self) -> Result<Vec<String>, Self::Error> {
        let mut conn = self.connection_pool.acquire().await?;
        dao::select_all_distinct_tags(&mut conn).await
    }

    async fn get_file(&self, guid: &str) -> Result<Option<File>, Self::Error> {
        let mut conn = self.connection_pool.acquire().await?;
        let Some(file) = dao::select_file_by_guid(&mut conn, guid).await? else {
            return Ok(None);
        };
        let tags = dao::select_content_tags_by_fingerprint(&mut conn, &file.fingerprint).await?;
        Ok(Some((file, tags).into()))
    }

    async fn update_file(&self, file: File) -> Result<(), Self::Error> {
        let mut tx = self.connection_pool.begin().await?;

        let Some(existing) = dao::select_file_by_guid(&mut tx, &file.guid).await? else {
            return Ok(());
        };

        // If the fingerprint changed, ensure the new content row exists first.
        if existing.fingerprint != file.fingerprint {
            dao::upsert_content(&mut tx, &file.fingerprint).await?;
        }

        // Update file-level fields (path, type, size, fingerprint).
        let updated = crate::db::models::File {
            id: existing.id,
            guid: existing.guid.clone(),
            path: file.path.clone(),
            type_: file.type_.clone(),
            size: file.size,
            fingerprint: file.fingerprint.clone(),
            status: existing.status,
            document_guid: existing.document_guid.clone(),
        };
        dao::update_file(&mut tx, &updated).await?;

        // Update content status.
        dao::update_content_status(&mut tx, &file.fingerprint, file.status.into()).await?;

        // Sync content tags: delete removed, upsert added.
        let existing_tags =
            dao::select_content_tags_by_fingerprint(&mut tx, &file.fingerprint).await?;
        let to_delete: Vec<String> = existing_tags
            .iter()
            .filter(|t| !file.tags.contains(&t.tag))
            .map(|t| t.tag.clone())
            .collect();
        dao::delete_content_tags(&mut tx, &file.fingerprint, to_delete).await?;
        let to_add: Vec<ContentTag> = file
            .tags
            .iter()
            .filter(|t| !existing_tags.iter().any(|e| &e.tag == *t))
            .map(|t| ContentTag::new(file.fingerprint.clone(), t.clone()))
            .collect();
        dao::upsert_many_content_tags(&mut tx, to_add).await?;

        tx.commit().await?;
        Ok(())
    }

    async fn get_file_tags(&self, guid: &str) -> Result<Vec<String>, Self::Error> {
        let mut conn = self.connection_pool.acquire().await?;
        let Some(file) = dao::select_file_by_guid(&mut conn, guid).await? else {
            return Ok(vec![]);
        };
        let tags = dao::select_content_tags_by_fingerprint(&mut conn, &file.fingerprint).await?;
        Ok(tags.into_iter().map(|t| t.tag).collect())
    }

    async fn add_file_tags(
        &self,
        guid: &str,
        tags: Vec<String>,
    ) -> Result<Vec<String>, Self::Error> {
        let mut conn = self.connection_pool.acquire().await?;
        let Some(file) = dao::select_file_by_guid(&mut conn, guid).await? else {
            return Ok(vec![]);
        };
        let content_tags: Vec<ContentTag> = tags
            .into_iter()
            .map(|tag| ContentTag::new(file.fingerprint.clone(), tag))
            .collect();
        dao::upsert_many_content_tags(&mut conn, content_tags).await?;
        let result = dao::select_content_tags_by_fingerprint(&mut conn, &file.fingerprint)
            .await?
            .into_iter()
            .map(|t| t.tag)
            .collect();
        Ok(result)
    }

    async fn delete_file_tags(&self, guid: &str, tags: Vec<String>) -> Result<(), Self::Error> {
        let mut conn = self.connection_pool.acquire().await?;
        let Some(file) = dao::select_file_by_guid(&mut conn, guid).await? else {
            return Ok(());
        };
        dao::delete_content_tags(&mut conn, &file.fingerprint, tags).await
    }

    async fn xdg_open_file(&self, file: File) -> Result<ExitStatus, Self::Error> {
        let status = Command::new("xdg-open").arg(file.path).status().await?;
        Ok(status)
    }

    async fn delete_file(&self, file: File) -> Result<(), Self::Error> {
        if let Err(e) = tokio::fs::remove_file(&file.path).await {
            tracing::warn!("Failed to delete file from filesystem: {}", e);
            return Err(Error::IO(Arc::new(e)));
        }
        let mut conn = self.connection_pool.acquire().await?;
        if let Some(db_file) = dao::select_file_by_guid(&mut conn, &file.guid).await? {
            dao::delete_file_record(&self.connection_pool, db_file.id).await?;
        }
        Ok(())
    }

    async fn get_reading_progress(
        &self,
        fingerprint: &str,
    ) -> Result<Option<ReadingProgress>, Self::Error> {
        let mut conn = self.connection_pool.acquire().await?;
        dao::get_reading_progress(&mut conn, fingerprint).await
    }

    async fn upsert_reading_progress(&self, progress: ReadingProgress) -> Result<(), Self::Error> {
        let mut conn = self.connection_pool.acquire().await?;
        dao::upsert_reading_progress(&mut conn, progress).await
    }

    async fn import_file(&self, path: &Path) -> Result<File, Self::Error> {
        let fingerprint = compute_sha256(path)
            .await
            .map_err(|e| Error::IO(Arc::new(e)))?;

        let metadata = tokio::fs::metadata(path)
            .await
            .map_err(|e| Error::IO(Arc::new(e)))?;
        let size: i32 = metadata
            .len()
            .try_into()
            .expect("file size too large for i32");

        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        let path_str = path.display().to_string();
        let mut conn = self.connection_pool.acquire().await?;

        dao::upsert_content(&mut conn, &fingerprint).await?;
        dao::upsert_file(
            &mut conn,
            NewFile {
                guid: uuid::Uuid::new_v4().to_string(),
                path: path_str.clone(),
                type_: extension,
                size,
                fingerprint: fingerprint.clone(),
            },
        )
        .await?;

        let db_file = dao::select_file_by_path(&mut conn, &path_str)
            .await?
            .expect("file should exist after upsert");
        let tags = dao::select_content_tags_by_fingerprint(&mut conn, &db_file.fingerprint).await?;
        Ok((db_file, tags).into())
    }
}

impl DbClient {
    pub async fn get_content_metadata(
        &self,
        fingerprint: &str,
    ) -> Result<Option<ContentMetadata>, Error> {
        let mut conn = self.connection_pool.acquire().await?;
        dao::select_content_metadata(&mut conn, fingerprint).await
    }
}

/// Wraps a [`DbClient`] and filters out files/tags whose tags include any of
/// the configured hidden (private) tags. Constructed via
/// [`ApplicationModule::filtered_db_client`].
#[derive(Clone)]
pub struct FilteredDbClient {
    inner: DbClient,
    hidden_tags: Vec<String>,
}

impl FilteredDbClient {
    pub fn new(inner: DbClient, hidden_tags: Vec<String>) -> Self {
        Self { inner, hidden_tags }
    }
}

#[async_trait::async_trait]
impl FileDataSource for FilteredDbClient {
    type Error = Error;

    fn display_name(&self) -> String {
        self.inner.display_name()
    }

    async fn status(&self) -> Result<Status, Self::Error> {
        self.inner.status().await
    }

    async fn get_files(&self) -> Result<Vec<File>, Self::Error> {
        let files = self.inner.get_files().await?;
        Ok(files
            .into_iter()
            .filter(|f| !f.tags.iter().any(|t| self.hidden_tags.contains(t)))
            .collect())
    }

    async fn get_files_tags(&self) -> Result<Vec<String>, Self::Error> {
        let tags = self.inner.get_files_tags().await?;
        Ok(tags
            .into_iter()
            .filter(|t| !self.hidden_tags.contains(t))
            .collect())
    }

    async fn get_file(&self, guid: &str) -> Result<Option<File>, Self::Error> {
        self.inner.get_file(guid).await
    }

    async fn get_file_tags(&self, guid: &str) -> Result<Vec<String>, Self::Error> {
        self.inner.get_file_tags(guid).await
    }

    async fn add_file_tags(
        &self,
        guid: &str,
        tags: Vec<String>,
    ) -> Result<Vec<String>, Self::Error> {
        self.inner.add_file_tags(guid, tags).await
    }

    async fn delete_file_tags(&self, guid: &str, tags: Vec<String>) -> Result<(), Self::Error> {
        self.inner.delete_file_tags(guid, tags).await
    }

    async fn update_file(&self, file: File) -> Result<(), Self::Error> {
        self.inner.update_file(file).await
    }

    async fn xdg_open_file(&self, file: File) -> Result<ExitStatus, Self::Error> {
        self.inner.xdg_open_file(file).await
    }

    async fn delete_file(&self, file: File) -> Result<(), Self::Error> {
        self.inner.delete_file(file).await
    }

    async fn import_file(&self, path: &Path) -> Result<File, Self::Error> {
        self.inner.import_file(path).await
    }

    async fn get_reading_progress(
        &self,
        fingerprint: &str,
    ) -> Result<Option<ReadingProgress>, Self::Error> {
        self.inner.get_reading_progress(fingerprint).await
    }

    async fn upsert_reading_progress(&self, progress: ReadingProgress) -> Result<(), Self::Error> {
        self.inner.upsert_reading_progress(progress).await
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
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect())
}
