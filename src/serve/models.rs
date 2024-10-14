use serde::{Deserialize, Serialize}; // TODO: extract into a separate web API crate

use crate::db::models::{File as DbFile, FileTag as DbTag};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct File {
    pub(super) id: i32,
    pub(super) path: String,
    pub(super) type_: String,
    pub(super) size: i32,
    pub(super) sha256sum: String,
    pub(super) tags: Vec<String>,
}

impl From<(DbFile, Vec<DbTag>)> for File {
    fn from((file, tags): (DbFile, Vec<DbTag>)) -> Self {
        let tags = tags.into_iter().map(|t| t.tag).collect();
        let DbFile {
            id,
            path,
            type_,
            size,
            sha256sum,
        } = file;
        Self {
            id,
            path,
            type_,
            size,
            sha256sum,
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
            sha256sum,
            tags,
        }: File,
    ) -> Self {
        let tags = tags
            .into_iter()
            .map(|tag| DbTag { file_id: id, tag })
            .collect();
        let file = DbFile {
            id,
            path,
            type_,
            size,
            sha256sum,
        };
        (file, tags)
    }
}
