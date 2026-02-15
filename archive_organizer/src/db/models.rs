use diesel::prelude::*;
use diesel::sqlite::Sqlite;
use serde::Deserialize;
use serde::Serialize;

use crate::db::schema::directories;
use crate::db::schema::directory_tags;
use crate::db::schema::file_tags;
use crate::db::schema::files;
use crate::db::schema::reading_progress;
use crate::db::schema::remotes;

#[derive(Debug, Clone, PartialEq, Eq, Queryable, Identifiable, Selectable)]
#[diesel(table_name = files)]
#[diesel(check_for_backend(Sqlite))]
pub struct File {
    pub id: i32,
    pub path: String,
    pub type_: String,
    pub size: i32,
    pub fingerprint: String,
    pub status: i32,
}

#[derive(Insertable)]
#[diesel(table_name = files)]
#[diesel(check_for_backend(Sqlite))]
pub struct NewFile {
    pub path: String,
    pub type_: String,
    pub size: i32,
    pub fingerprint: String,
    pub status: i32,
}

#[derive(Debug, Default, Identifiable, AsChangeset)]
#[diesel(table_name = files)]
#[diesel(check_for_backend(Sqlite))]
pub struct UpdateFile {
    pub id: i32,
    pub path: Option<String>,
    pub type_: Option<String>,
    pub size: Option<i32>,
    pub fingerprint: Option<String>,
    pub status: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Queryable, Selectable, Insertable, Associations)]
#[diesel(belongs_to(File))]
#[diesel(table_name = file_tags)]
#[diesel(check_for_backend(Sqlite))]
pub struct FileTag {
    pub file_id: i32,
    pub tag: String,
}

impl FileTag {
    pub fn new(file_id: i32, tag: String) -> Self {
        Self { file_id, tag }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Queryable, Identifiable, Selectable)]
#[diesel(table_name = directories)]
#[diesel(check_for_backend(Sqlite))]
pub struct Directory {
    pub id: i32,
    pub path: String,
    pub type_: String,
}

#[derive(Insertable)]
#[diesel(table_name = directories)]
#[diesel(check_for_backend(Sqlite))]
pub struct NewDirectory {
    pub path: String,
    pub type_: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Queryable, Selectable, Insertable, Associations)]
#[diesel(belongs_to(Directory))]
#[diesel(table_name = directory_tags)]
#[diesel(check_for_backend(Sqlite))]
pub struct DirectoryTag {
    pub directory_id: i32,
    pub tag: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Queryable, Identifiable, Selectable)]
#[diesel(table_name = remotes)]
#[diesel(check_for_backend(Sqlite))]
pub struct Remote {
    pub id: i32,
    pub base_url: String,
    pub order: i32,
    pub passphrase: String,
    pub user_id: String,
}

#[derive(Insertable)]
#[diesel(table_name = remotes)]
#[diesel(check_for_backend(Sqlite))]
pub struct NewRemote {
    pub base_url: String,
    pub order: i32,
    pub passphrase: String,
    pub user_id: String,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    Queryable,
    Identifiable,
    Selectable,
    Insertable,
    AsChangeset,
)]
#[diesel(table_name = reading_progress)]
#[diesel(primary_key(fingerprint))]
#[diesel(check_for_backend(Sqlite))]
pub struct ReadingProgress {
    pub fingerprint: String,
    pub progress: String,
    pub last_updated: String,
}
