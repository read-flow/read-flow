use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::process::ExitStatus;
use std::result::Result;
use std::str::FromStr;

use serde::Deserialize;
use serde::Serialize;
use strum::EnumIter;

use crate::db::models::ContentTag;
use crate::db::models::File as DbFile;
pub use crate::db::models::ReadingProgress;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, PartialOrd, Ord, EnumIter, Hash,
)]
pub enum ReadingStatus {
    Unread,
    Reading,
    Read,
}

impl fmt::Display for ReadingStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl FromStr for ReadingStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "unread" => Ok(Self::Unread),
            "reading" => Ok(Self::Reading),
            "read" => Ok(Self::Read),
            _ => Err(format!("Invalid reading status: {s}")),
        }
    }
}

// TODO: should be TryFrom
impl From<i32> for ReadingStatus {
    fn from(value: i32) -> Self {
        match value {
            0 => Self::Unread,
            1 => Self::Reading,
            2 => Self::Read,
            _ => panic!("Invalid file status"),
        }
    }
}

impl From<ReadingStatus> for i32 {
    fn from(value: ReadingStatus) -> Self {
        match value {
            ReadingStatus::Unread => 0,
            ReadingStatus::Reading => 1,
            ReadingStatus::Read => 2,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Status {
    pub identifier: String,
    #[serde(flatten)]
    pub attributes: HashMap<String, String>,
    pub nested_checks: Vec<Status>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct File {
    pub guid: String,
    pub path: String,
    pub type_: String,
    pub size: i32,
    pub fingerprint: String,
    pub tags: Vec<String>,
    pub status: ReadingStatus,
}

impl From<(DbFile, Vec<ContentTag>)> for File {
    fn from((file, tags): (DbFile, Vec<ContentTag>)) -> Self {
        let tags = tags.into_iter().map(|t| t.tag).collect();
        let DbFile {
            guid,
            path,
            type_,
            size,
            fingerprint,
            status,
            ..
        } = file;
        Self {
            guid,
            path,
            type_,
            size,
            fingerprint,
            tags,
            status: status.into(),
        }
    }
}

#[async_trait::async_trait]
pub trait FileDataSource {
    type Error: std::error::Error;

    /// Returns a display name for this data source.
    ///
    /// This is used for UI elements like tab labels and headers.
    /// For local data sources, this should return "Local Files".
    /// For remote data sources, this should return "Remote: hostname".
    fn display_name(&self) -> String;

    async fn status(&self) -> Result<Status, Self::Error>;

    async fn get_files(&self) -> Result<Vec<File>, Self::Error>;

    async fn get_files_tags(&self) -> Result<Vec<String>, Self::Error>;

    async fn get_file(&self, guid: &str) -> Result<Option<File>, Self::Error>;

    async fn get_file_tags(&self, guid: &str) -> Result<Vec<String>, Self::Error>;

    async fn add_file_tags(
        &self,
        guid: &str,
        tags: Vec<String>,
    ) -> Result<Vec<String>, Self::Error>;

    async fn delete_file_tags(&self, guid: &str, tags: Vec<String>) -> Result<(), Self::Error>;

    async fn update_file(&self, file: File) -> Result<(), Self::Error>;

    async fn xdg_open_file(&self, file: File) -> Result<ExitStatus, Self::Error>;

    /// Delete a file both from the database and the filesystem.
    async fn delete_file(&self, file: File) -> Result<(), Self::Error>;

    /// Import a file from a local filesystem path into this data source.
    ///
    /// For local data sources, this registers the file in the database.
    /// For remote data sources, this uploads the file to the server.
    async fn import_file(&self, path: &Path) -> Result<File, Self::Error>;

    async fn get_reading_progress(
        &self,
        fingerprint: &str,
    ) -> Result<Option<ReadingProgress>, Self::Error>;

    async fn upsert_reading_progress(&self, progress: ReadingProgress) -> Result<(), Self::Error>;
}
