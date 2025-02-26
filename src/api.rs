use std::{
    fmt::{Display, Formatter},
    process::ExitStatus,
    result::Result,
};

use serde::{Deserialize, Serialize};
use strum::EnumIter;

use crate::db::models::{File as DbFile, FileTag as DbTag};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, PartialOrd, Ord, EnumIter)]
pub enum FileStatus {
    Unread,
    Reading,
    Read,
}

impl Display for FileStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

// TODO: should be TryFrom
impl From<i32> for FileStatus {
    fn from(value: i32) -> Self {
        match value {
            0 => Self::Unread,
            1 => Self::Reading,
            2 => Self::Read,
            _ => panic!("Invalid file status"),
        }
    }
}

impl From<FileStatus> for i32 {
    fn from(value: FileStatus) -> Self {
        match value {
            FileStatus::Unread => 0,
            FileStatus::Reading => 1,
            FileStatus::Read => 2,
        }
    }
}

pub fn get_update<T: PartialEq + Clone>(original: &T, updated: &T) -> Option<T> {
    if original == updated {
        None
    } else {
        Some(updated.clone())
    }
}

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
    pub status: FileStatus,
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
            status,
        } = file;
        Self {
            id,
            path,
            type_,
            size,
            fingerprint,
            tags,
            status: status.into(),
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
            status,
        }: File,
    ) -> Self {
        let tags = tags.into_iter().map(|tag| DbTag::new(id, tag)).collect();
        let file = DbFile {
            id,
            path,
            type_,
            size,
            fingerprint,
            status: status.into(),
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

    async fn update_file(&self, file: File) -> Result<(), Self::Error>;

    async fn xdg_open_file(&self, file: File) -> Result<ExitStatus, Self::Error>;
}
