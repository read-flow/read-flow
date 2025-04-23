use std::{process::ExitStatus, sync::Arc};

use diesel::{RunQueryDsl, prelude::*};
use indexmap::IndexMap;
use tokio::process::Command;

use crate::{
    api::{File, FileDataSource, Status},
    db::models::{File as DbFile, FileTag as DbFileTag},
    db::schema::{file_tags, files},
};

use super::ConnectionPool;
use super::dao::{Error, FileDao, FileTagDao};

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
            let mut connection = self.connection_pool.get()?;
            let _: usize = diesel::sql_query("SELECT 1").execute(&mut connection)?;
            Ok(Status {})
        })
    }

    async fn get_files(&self) -> Result<Vec<File>, Self::Error> {
        tokio::task::block_in_place(|| {
            let files = self.connection_pool.select_all_files()?;
            let file_tags = self.connection_pool.select_all_file_tags()?;

            let mut result: IndexMap<i32, (DbFile, Vec<DbFileTag>)> = files
                .into_iter()
                .map(|file| (file.id, (file, Vec::new())))
                .collect();

            for tag in file_tags {
                if let Some((_file, tags)) = result.get_mut(&tag.file_id) {
                    tags.push(tag);
                }
            }

            let result = result.into_values().map(Into::into).collect();
            Ok(result)
        })
    }

    async fn get_files_tags(&self) -> Result<Vec<String>, Self::Error> {
        tokio::task::block_in_place(|| self.connection_pool.select_all_tags())
    }

    async fn get_file(&self, id: i32) -> Result<Option<File>, Self::Error> {
        tokio::task::block_in_place(|| {
            let file = self.connection_pool.select_file_by_id(id)?;
            let file_tags = self.connection_pool.select_file_tags_by_file_id(id)?;
            let result = file.map(|file| (file, file_tags).into());
            Ok(result)
        })
    }

    async fn update_file(&self, file: File) -> Result<(), Self::Error> {
        tokio::task::block_in_place(|| {
            let (file, tags) = file.into();
            let file_id = file.id;
            self.connection_pool.update_file(file)?;

            // Delete removed tags
            self.connection_pool
                .select_file_tags_by_file_id(file_id)?
                .into_iter()
                .map(|tag| {
                    if !tags.iter().any(|t| t.tag == tag.tag) {
                        self.connection_pool.delete_file_tag(tag)
                    } else {
                        Ok(())
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;

            // Insert any new tags
            self.connection_pool.upsert_many_file_tags(tags)?;
            Ok(())
        })
    }

    async fn get_file_tags(&self, id: i32) -> Result<Vec<String>, Self::Error> {
        tokio::task::block_in_place(|| {
            let file_tags = self.connection_pool.select_file_tags_by_file_id(id)?;
            let file_tags = file_tags.into_iter().map(|t| t.tag).collect();
            Ok(file_tags)
        })
    }

    async fn add_file_tags(&self, id: i32, tags: Vec<String>) -> Result<Vec<String>, Self::Error> {
        tokio::task::block_in_place(|| {
            let tags = tags
                .into_iter()
                .map(|tag| DbFileTag::new(id, tag))
                .collect();
            self.connection_pool.upsert_many_file_tags(tags)?;
            let tags = self
                .connection_pool
                .select_file_tags_by_file_id(id)?
                .into_iter()
                .map(|tag| tag.tag)
                .collect();
            Ok(tags)
        })
    }

    async fn delete_file_tags(&self, id: i32, tags: Vec<String>) -> Result<(), Self::Error> {
        tokio::task::block_in_place(|| {
            for tag in tags {
                self.connection_pool
                    .delete_file_tag(DbFileTag::new(id, tag))?;
            }
            Ok(())
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
            let mut connection = self.connection_pool.get()?;

            // First delete all tags associated with the file
            diesel::delete(file_tags::table.filter(file_tags::file_id.eq(file.id)))
                .execute(&mut connection)?;

            // Then delete the file itself
            diesel::delete(files::table.filter(files::id.eq(file.id))).execute(&mut connection)?;

            Ok(())
        })
    }
}
