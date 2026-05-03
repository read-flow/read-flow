use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct File {
    pub id: i32,
    pub guid: String,
    pub path: String,
    #[sqlx(rename = "type")]
    pub type_: String,
    pub size: i32,
    pub fingerprint: String,
    /// Joined from contents.status
    pub status: i32,
}

pub struct NewFile {
    pub guid: String,
    pub path: String,
    pub type_: String,
    pub size: i32,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Content {
    pub fingerprint: String,
    pub status: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ContentTag {
    pub fingerprint: String,
    pub tag: String,
}

impl ContentTag {
    pub fn new(fingerprint: String, tag: String) -> Self {
        Self { fingerprint, tag }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ContentMetadata {
    pub fingerprint: String,
    pub title: Option<String>,
    pub authors: Option<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub identifier: Option<String>,
    pub date: Option<String>,
    pub extracted_at: String,
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
