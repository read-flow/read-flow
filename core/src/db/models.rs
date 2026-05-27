use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Document {
    pub id: i32,
    pub guid: String,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    strum::EnumIter,
    strum::Display,
    strum::EnumString,
)]
#[serde(rename_all = "PascalCase")]
#[strum(serialize_all = "PascalCase")]
pub enum DocumentType {
    Book,
    Article,
    ResearchPaper,
    Thesis,
    Letter,
    Magazine,
    Manual,
    Report,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct DocumentUserMetadata {
    pub document_id: i32,
    pub document_type: Option<String>,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub authors: Option<String>,
    pub description: Option<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub identifier: Option<String>,
    pub date: Option<String>,
    pub subject: Option<String>,
    pub updated_at: String,
}

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
    /// Joined from documents.guid via contents.document_id (NULL when ungrouped)
    pub document_guid: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct ReadingState {
    pub fingerprint: String,
    pub status: i32,
    pub position: String,
    pub percentage: f64,
    pub last_updated: String,
    pub status_updated_at: String,
}
