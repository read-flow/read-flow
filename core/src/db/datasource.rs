use std::path::Path;
use std::process::ExitStatus;
use std::sync::Arc;

use diesel::prelude::*;
use tokio::process::Command;

use super::ConnectionPool;
use super::ConnectionPoolExt;
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
        tokio::task::block_in_place(|| {
            self.connection_pool.with_connection(|conn| {
                let _: usize = diesel::sql_query("SELECT 1").execute(conn)?;
                let status = Status {
                    identifier: "database".to_string(),
                    ..Default::default()
                };
                Ok(status)
            })
        })
    }

    async fn get_files(&self) -> Result<Vec<File>, Self::Error> {
        tokio::task::block_in_place(|| {
            self.connection_pool.with_connection(|conn| {
                let files = dao::select_all_files(conn)?;
                let file_tags = dao::select_all_file_tags(conn)?;

                let mut result: FxIndexMap<i32, (DbFile, Vec<DbFileTag>)> = files
                    .into_iter()
                    .map(|file| (file.id, (file, Vec::<DbFileTag>::new())))
                    .collect();

                for tag in file_tags {
                    if let Some((_file, tags)) = result.get_mut(&tag.file_id) {
                        tags.push(tag);
                    }
                }

                let result = result.into_values().map(Into::into).collect();
                Ok(result)
            })
        })
    }

    async fn get_files_tags(&self) -> Result<Vec<String>, Self::Error> {
        tokio::task::block_in_place(|| self.connection_pool.with_connection(dao::select_all_tags))
    }

    async fn get_file(&self, id: i32) -> Result<Option<File>, Self::Error> {
        tokio::task::block_in_place(|| {
            self.connection_pool.with_connection(|conn| {
                let file = dao::select_file_by_id(conn, id)?;
                let file_tags = dao::select_file_tags_by_file_id(conn, id)?;
                let result = file.map(|file| (file, file_tags).into());
                Ok(result)
            })
        })
    }

    async fn update_file(&self, file: File) -> Result<(), Self::Error> {
        tokio::task::block_in_place(|| {
            let (file, tags) = file.into();
            let file_id = file.id;
            self.connection_pool.with_connection(|conn| {
                dao::update_file(conn, file)?;

                // Delete removed tags
                let existing_tags = dao::select_file_tags_by_file_id(conn, file_id)?;
                for tag in existing_tags {
                    if !tags.iter().any(|t| t.tag == tag.tag) {
                        dao::delete_file_tag(conn, tag)?;
                    }
                }

                // Insert any new tags
                dao::upsert_many_file_tags(conn, tags)?;
                Ok(())
            })
        })
    }

    async fn get_file_tags(&self, id: i32) -> Result<Vec<String>, Self::Error> {
        tokio::task::block_in_place(|| {
            self.connection_pool.with_connection(|conn| {
                let file_tags = dao::select_file_tags_by_file_id(conn, id)?;
                let file_tags = file_tags.into_iter().map(|t| t.tag).collect();
                Ok(file_tags)
            })
        })
    }

    async fn add_file_tags(&self, id: i32, tags: Vec<String>) -> Result<Vec<String>, Self::Error> {
        tokio::task::block_in_place(|| {
            self.connection_pool.with_connection(|conn| {
                let db_tags: Vec<DbFileTag> = tags
                    .into_iter()
                    .map(|tag| DbFileTag::new(id, tag))
                    .collect();
                dao::upsert_many_file_tags(conn, db_tags)?;
                let result = dao::select_file_tags_by_file_id(conn, id)?
                    .into_iter()
                    .map(|tag| tag.tag)
                    .collect();
                Ok(result)
            })
        })
    }

    async fn delete_file_tags(&self, id: i32, tags: Vec<String>) -> Result<(), Self::Error> {
        tokio::task::block_in_place(|| {
            self.connection_pool.with_connection(|conn| {
                for tag in tags {
                    dao::delete_file_tag(conn, DbFileTag::new(id, tag))?;
                }
                Ok(())
            })
        })
    }

    async fn xdg_open_file(&self, file: File) -> Result<ExitStatus, Self::Error> {
        let status = Command::new("xdg-open").arg(file.path).status().await?;
        Ok(status)
    }

    async fn delete_file(&self, file: File) -> Result<(), Self::Error> {
        // First delete the file from the filesystem
        if let Err(e) = tokio::fs::remove_file(&file.path).await {
            tracing::warn!("Failed to delete file from filesystem: {}", e);
            return Err(Error::IO(Arc::new(e)));
        }

        // Then delete the file from the database
        tokio::task::block_in_place(|| {
            self.connection_pool
                .with_connection(|conn| dao::delete_file_record(conn, file.id))
        })
    }

    async fn get_reading_progress(
        &self,
        fingerprint: &str,
    ) -> Result<Option<ReadingProgress>, Self::Error> {
        let fingerprint = fingerprint.to_string();
        tokio::task::block_in_place(|| {
            self.connection_pool
                .with_connection(|conn| dao::get_reading_progress(conn, &fingerprint))
        })
    }

    async fn upsert_reading_progress(&self, progress: ReadingProgress) -> Result<(), Self::Error> {
        tokio::task::block_in_place(|| {
            self.connection_pool
                .with_connection(|conn| dao::upsert_reading_progress(conn, progress))
        })
    }

    async fn import_file(&self, path: &Path) -> Result<File, Self::Error> {
        // Compute SHA256 fingerprint
        let output = Command::new("sha256sum")
            .arg(path)
            .output()
            .await
            .map_err(|e| Error::IO(Arc::new(e)))?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let fingerprint = stdout
            .split(' ')
            .next()
            .expect("expected fingerprint")
            .to_string();

        // Get file metadata
        let metadata = tokio::fs::metadata(path)
            .await
            .map_err(|e| Error::IO(Arc::new(e)))?;
        let size: i32 = metadata
            .len()
            .try_into()
            .expect("file size too large for i32");

        // Get extension
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

        // Upsert into database and fetch back
        tokio::task::block_in_place(|| {
            self.connection_pool.with_connection(|conn| {
                dao::upsert_file(conn, new_file)?;
                let db_file = dao::select_file_by_path(conn, &path_str)?
                    .expect("file should exist after upsert");
                let file_tags = dao::select_file_tags_by_file_id(conn, db_file.id)?;
                Ok((db_file, file_tags).into())
            })
        })
    }
}
