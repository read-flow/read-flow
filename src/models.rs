use diesel::{prelude::*, sqlite::Sqlite};
use serde::{Deserialize, Serialize}; // TODO: extract into a separate web API crate

use crate::schema::{directories, directory_tags, file_tags, files};

#[derive(
    Debug, Clone, PartialEq, Eq, Queryable, Identifiable, Selectable, Serialize, Deserialize,
)]
#[diesel(table_name = files)]
#[diesel(check_for_backend(Sqlite))]
pub struct File {
    pub id: i32,
    pub path: String,
    pub type_: String,
    pub size: i32,
    pub sha256sum: String,
}

#[derive(Insertable)]
#[diesel(table_name = files)]
#[diesel(check_for_backend(Sqlite))]
pub struct NewFile {
    pub path: String,
    pub type_: String,
    pub size: i32,
    pub sha256sum: String,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Queryable,
    Selectable,
    Insertable,
    Associations,
    Serialize,
    Deserialize,
)]
#[diesel(belongs_to(File))]
#[diesel(table_name = file_tags)]
#[diesel(check_for_backend(Sqlite))]
pub struct FileTag {
    pub file_id: i32,
    pub tag: String,
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
