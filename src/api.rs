use std::{process::ExitStatus, result::Result};

use serde::{Deserialize, Serialize};

use crate::db::models::{File as DbFile, FileTag as DbTag};

#[derive(Debug, Serialize, Deserialize)]
pub struct Status {}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct File {
    pub id: i32,
    pub path: String,
    pub type_: String,
    pub size: i32,
    pub fingerprint: String,
    pub tags: Vec<String>,
}

impl From<(DbFile, Vec<DbTag>)> for File {
    fn from((file, tags): (DbFile, Vec<DbTag>)) -> Self {
        let tags = tags.into_iter().map(|t| t.tag).collect();
        let DbFile {
            id,
            path,
            type_,
            size,
            fingerprint,
        } = file;
        Self {
            id,
            path,
            type_,
            size,
            fingerprint,
            tags,
        }
    }
}

impl From<File> for (DbFile, Vec<DbTag>) {
    fn from(
        File {
            id,
            path,
            type_,
            size,
            fingerprint,
            tags,
        }: File,
    ) -> Self {
        let tags = tags.into_iter().map(|tag| DbTag::new(id, tag)).collect();
        let file = DbFile {
            id,
            path,
            type_,
            size,
            fingerprint,
        };
        (file, tags)
    }
}

#[async_trait::async_trait]
pub trait FileDataSource {
    type Error: std::error::Error;

    async fn status(&self) -> Result<Status, Self::Error>;

    async fn get_files(&self) -> Result<Vec<File>, Self::Error>;

    async fn get_files_tags(&self) -> Result<Vec<String>, Self::Error>;

    async fn get_file(&self, id: i32) -> Result<Option<File>, Self::Error>;

    async fn get_file_tags(&self, id: i32) -> Result<Vec<String>, Self::Error>;

    async fn add_file_tags(&self, id: i32, tags: Vec<String>) -> Result<Vec<String>, Self::Error>;

    async fn delete_file_tags(&self, id: i32, tags: Vec<String>) -> Result<(), Self::Error>;

    async fn xdg_open_file(&self, file: File) -> Result<ExitStatus, Self::Error>;
}
