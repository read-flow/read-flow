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
use crate::FxIndexMap;
use crate::api::File;
use crate::api::FileDataSource;
use crate::api::ReadingProgress;
use crate::api::ReadingStatus;
use crate::api::Status;
use crate::db::models::File as DbFile;
use crate::db::models::FileTag as DbFileTag;
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
        let files = dao::select_all_files(&mut *conn).await?;
        let file_tags = dao::select_all_file_tags(&mut *conn).await?;

        let mut result: FxIndexMap<i32, (DbFile, Vec<DbFileTag>)> = files
            .into_iter()
            .map(|file| (file.id, (file, Vec::<DbFileTag>::new())))
            .collect();

        for tag in file_tags {
            if let Some((_file, tags)) = result.get_mut(&tag.file_id) {
                tags.push(tag);
            }
        }

        Ok(result.into_values().map(Into::into).collect())
    }

    async fn get_files_tags(&self) -> Result<Vec<String>, Self::Error> {
        let mut conn = self.connection_pool.acquire().await?;
        dao::select_all_tags(&mut *conn).await
    }

    async fn get_file(&self, id: i32) -> Result<Option<File>, Self::Error> {
        let mut conn = self.connection_pool.acquire().await?;
        let file = dao::select_file_by_id(&mut *conn, id).await?;
        let file_tags = dao::select_file_tags_by_file_id(&mut *conn, id).await?;
        Ok(file.map(|file| (file, file_tags).into()))
    }

    async fn update_file(&self, file: File) -> Result<(), Self::Error> {
        let (db_file, tags) = file.into();
        let file_id = db_file.id;
        let mut tx = self.connection_pool.begin().await?;
        dao::update_file(&mut *tx, db_file).await?;
        let existing_tags = dao::select_file_tags_by_file_id(&mut *tx, file_id).await?;
        let tags_to_delete: Vec<String> = existing_tags
            .into_iter()
            .filter(|tag| !tags.iter().any(|t| t.tag == tag.tag))
            .map(|tag| tag.tag)
            .collect();
        dao::delete_file_tags(&mut *tx, file_id, tags_to_delete).await?;
        dao::upsert_many_file_tags(&mut *tx, tags).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn get_file_tags(&self, id: i32) -> Result<Vec<String>, Self::Error> {
        let mut conn = self.connection_pool.acquire().await?;
        let file_tags = dao::select_file_tags_by_file_id(&mut *conn, id).await?;
        Ok(file_tags.into_iter().map(|t| t.tag).collect())
    }

    async fn add_file_tags(&self, id: i32, tags: Vec<String>) -> Result<Vec<String>, Self::Error> {
        let db_tags: Vec<DbFileTag> = tags
            .into_iter()
            .map(|tag| DbFileTag::new(id, tag))
            .collect();
        let mut conn = self.connection_pool.acquire().await?;
        dao::upsert_many_file_tags(&mut *conn, db_tags).await?;
        let result = dao::select_file_tags_by_file_id(&mut *conn, id)
            .await?
            .into_iter()
            .map(|tag| tag.tag)
            .collect();
        Ok(result)
    }

    async fn delete_file_tags(&self, id: i32, tags: Vec<String>) -> Result<(), Self::Error> {
        let mut conn = self.connection_pool.acquire().await?;
        dao::delete_file_tags(&mut *conn, id, tags).await
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
        dao::delete_file_record(&self.connection_pool, file.id).await
    }

    async fn get_reading_progress(
        &self,
        fingerprint: &str,
    ) -> Result<Option<ReadingProgress>, Self::Error> {
        let mut conn = self.connection_pool.acquire().await?;
        dao::get_reading_progress(&mut *conn, fingerprint).await
    }

    async fn upsert_reading_progress(&self, progress: ReadingProgress) -> Result<(), Self::Error> {
        let mut conn = self.connection_pool.acquire().await?;
        dao::upsert_reading_progress(&mut *conn, progress).await
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
        let new_file = NewFile {
            path: path_str.clone(),
            type_: extension,
            size,
            fingerprint,
            status: ReadingStatus::Unread.into(),
        };

        let mut conn = self.connection_pool.acquire().await?;
        dao::upsert_file(&mut *conn, new_file).await?;
        let db_file = dao::select_file_by_path(&mut *conn, &path_str)
            .await?
            .expect("file should exist after upsert");
        let file_tags = dao::select_file_tags_by_file_id(&mut *conn, db_file.id).await?;
        Ok((db_file, file_tags).into())
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
