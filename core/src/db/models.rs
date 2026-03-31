use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct File {
    pub id: i32,
    pub path: String,
    #[sqlx(rename = "type")]
    pub type_: String,
    pub size: i32,
    pub fingerprint: String,
    pub status: i32,
}

pub struct NewFile {
    pub path: String,
    pub type_: String,
    pub size: i32,
    pub fingerprint: String,
    pub status: i32,
}

pub struct UpdateFile {
    pub id: i32,
    pub path: Option<String>,
    pub type_: Option<String>,
    pub size: Option<i32>,
    pub fingerprint: Option<String>,
    pub status: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct FileTag {
    pub file_id: i32,
    pub tag: String,
}

impl FileTag {
    pub fn new(file_id: i32, tag: String) -> Self {
        Self { file_id, tag }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Directory {
    pub id: i32,
    pub path: String,
    #[sqlx(rename = "type")]
    pub type_: String,
}

pub struct NewDirectory {
    pub path: String,
    pub type_: String,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct DirectoryTag {
    pub directory_id: i32,
    pub tag: String,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Remote {
    pub id: i32,
    pub base_url: String,
    pub order: i32,
    pub passphrase: String,
    pub user_id: String,
}

pub struct NewRemote {
    pub base_url: String,
    pub order: i32,
    pub passphrase: String,
    pub user_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct ReadingProgress {
    pub fingerprint: String,
    pub progress: String,
    pub last_updated: String,
}
