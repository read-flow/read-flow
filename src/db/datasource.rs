use std::process::ExitStatus;

use diesel::RunQueryDsl;
use indexmap::IndexMap;
use tokio::process::Command;

use crate::{
    api::{File, FileDataSource, Status},
    db::models::{File as DbFile, FileTag as DbFileTag},
};

use super::dao::{Error, FileDao, FileTagDao};
use super::ConnectionPool;

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
}
