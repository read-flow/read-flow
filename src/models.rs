use diesel::prelude::*;

#[derive(Queryable, Selectable)]
#[diesel(table_name = crate::schema::files)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct File {
    pub id: i32,
    pub path: String,
    pub type_: String,
    pub size: i32,
    pub sha256sum: String,
}

#[derive(Insertable)]
#[diesel(table_name = crate::schema::files)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct NewFile {
    pub path: String,
    pub type_: String,
    pub size: i32,
    pub sha256sum: String,
}

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::file_tags)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct FileTag {
    pub file_id: i32,
    pub tag: String,
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = crate::schema::directories)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Directory {
    pub id: i32,
    pub path: String,
    pub type_: String,
}

#[derive(Insertable)]
#[diesel(table_name = crate::schema::directories)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct NewDirectory {
    pub path: String,
    pub type_: String,
}

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::directory_tags)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct DirectoryTag {
    pub directory_id: i32,
    pub tag: String,
}
