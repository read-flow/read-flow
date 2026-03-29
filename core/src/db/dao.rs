use std::io;
use std::sync::Arc;

use diesel::Connection;
use diesel::connection::LoadConnection;
use diesel::prelude::*;
use diesel::sql_query;
use diesel::sql_types::Integer;
use diesel::sql_types::Text;
use diesel::sqlite::Sqlite;

use crate::api::get_update;
use crate::db::models::Directory;
use crate::db::models::File;
use crate::db::models::FileTag;
use crate::db::models::NewDirectory;
use crate::db::models::NewFile;
use crate::db::models::NewRemote;
use crate::db::models::ReadingProgress;
use crate::db::models::Remote;
use crate::db::models::UpdateFile;
use crate::db::schema::directories;
use crate::db::schema::file_tags;
use crate::db::schema::files;
use crate::db::schema::reading_progress;
use crate::db::schema::remotes;

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Diesel(#[source] Arc<diesel::result::Error>),
    #[error("connection pool error: {0}")]
    R2D2(#[source] Arc<r2d2::Error>),
    #[error("io error: {0}")]
    IO(#[source] Arc<io::Error>),
}

impl From<diesel::result::Error> for Error {
    fn from(value: diesel::result::Error) -> Self {
        Self::Diesel(Arc::new(value))
    }
}

impl From<r2d2::Error> for Error {
    fn from(value: r2d2::Error) -> Self {
        Self::R2D2(Arc::new(value))
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::IO(Arc::new(value))
    }
}

/// Supertrait for connections accepted by all DAO functions.
/// Centralises the backend constraint so individual functions only bound on
/// `DaoConnection` rather than repeating `Connection<Backend = Sqlite> +
/// LoadConnection` everywhere.
pub trait DaoConnection: Connection<Backend = Sqlite> + LoadConnection {}

impl<C> DaoConnection for C where C: Connection<Backend = Sqlite> + LoadConnection {}

pub fn insert_file<C>(conn: &mut C, file: NewFile) -> Result<File, Error>
where
    C: DaoConnection,
{
    let file = diesel::insert_into(files::table)
        .values(&file)
        .returning(File::as_returning())
        .get_result(conn)?;
    Ok(file)
}

pub fn upsert_file<C>(conn: &mut C, file: NewFile) -> Result<(), Error>
where
    C: DaoConnection,
{
    diesel::insert_into(files::table)
        .values(&file)
        .on_conflict_do_nothing()
        .execute(conn)?;
    Ok(())
}

pub fn update_file<C>(conn: &mut C, file: File) -> Result<(), Error>
where
    C: DaoConnection,
{
    let original_file = files::table
        .find(file.id)
        .select(File::as_select())
        .first(conn)?;

    if original_file != file {
        let update_file = UpdateFile {
            id: file.id,
            path: get_update(&original_file.path, &file.path),
            type_: get_update(&original_file.type_, &file.type_),
            size: get_update(&original_file.size, &file.size),
            fingerprint: get_update(&original_file.fingerprint, &file.fingerprint),
            status: get_update(&original_file.status, &file.status),
        };

        tracing::debug!("Updating file: {update_file:?}");

        diesel::update(files::table)
            .filter(files::id.eq(file.id))
            .set(update_file)
            .execute(conn)?;
    }
    Ok(())
}

pub fn insert_many_files<C>(conn: &mut C, files: Vec<NewFile>) -> Result<(), Error>
where
    C: DaoConnection,
{
    for file in files {
        insert_file(conn, file)?;
    }
    Ok(())
}

pub fn upsert_many_files<C>(conn: &mut C, files: Vec<NewFile>) -> Result<(), Error>
where
    C: DaoConnection,
{
    for file in files {
        upsert_file(conn, file)?;
    }
    Ok(())
}

pub fn select_all_files<C>(conn: &mut C) -> Result<Vec<File>, Error>
where
    C: DaoConnection,
{
    let files = files::table.load(conn)?;
    Ok(files)
}

pub fn select_all_files_order_by_id<C>(conn: &mut C) -> Result<Vec<File>, Error>
where
    C: DaoConnection,
{
    let files = files::table.order_by(files::columns::id).load(conn)?;
    Ok(files)
}

pub fn select_all_files_order_by_type<C>(conn: &mut C) -> Result<Vec<File>, Error>
where
    C: DaoConnection,
{
    let files = files::table.order_by(files::columns::type_).load(conn)?;
    Ok(files)
}

pub fn select_all_files_order_by_path<C>(conn: &mut C) -> Result<Vec<File>, Error>
where
    C: DaoConnection,
{
    let files = files::table.order_by(files::columns::path).load(conn)?;
    Ok(files)
}

pub fn select_all_files_order_by_size<C>(conn: &mut C) -> Result<Vec<File>, Error>
where
    C: DaoConnection,
{
    let files = files::table.order_by(files::columns::size).load(conn)?;
    Ok(files)
}

pub fn select_all_files_order_by_fingerprint<C>(conn: &mut C) -> Result<Vec<File>, Error>
where
    C: DaoConnection,
{
    let files = files::table
        .order_by(files::columns::fingerprint)
        .load(conn)?;
    Ok(files)
}

pub fn select_file_by_id<C>(conn: &mut C, id: i32) -> Result<Option<File>, Error>
where
    C: DaoConnection,
{
    let file = files::table
        .find(id)
        .select(File::as_select())
        .first(conn)
        .optional()?;
    Ok(file)
}

pub fn select_file_by_path<C>(conn: &mut C, path: &str) -> Result<Option<File>, Error>
where
    C: DaoConnection,
{
    let file = files::table
        .filter(files::path.eq(path))
        .select(File::as_select())
        .first(conn)
        .optional()?;
    Ok(file)
}

pub fn select_all_files_by_path_like<C>(conn: &mut C, path: &str) -> Result<Vec<File>, Error>
where
    C: DaoConnection,
{
    let files = files::table.filter(files::path.like(path)).load(conn)?;
    Ok(files)
}

pub fn delete_file_record<C>(conn: &mut C, id: i32) -> Result<(), Error>
where
    C: DaoConnection,
{
    diesel::delete(file_tags::table.filter(file_tags::file_id.eq(id))).execute(conn)?;
    diesel::delete(files::table.filter(files::id.eq(id))).execute(conn)?;
    Ok(())
}

pub fn insert_file_tag<C>(conn: &mut C, file_tag: FileTag) -> Result<FileTag, Error>
where
    C: DaoConnection,
{
    tracing::debug!("inserting tag: {file_tag:?}");
    let file_tag = diesel::insert_into(file_tags::table)
        .values(&file_tag)
        .returning(FileTag::as_returning())
        .get_result(conn)?;
    Ok(file_tag)
}

pub fn upsert_file_tag<C>(conn: &mut C, file_tag: FileTag) -> Result<(), Error>
where
    C: DaoConnection,
{
    tracing::debug!("upserting tag: {file_tag:?}");
    diesel::insert_into(file_tags::table)
        .values(&file_tag)
        .on_conflict_do_nothing()
        .execute(conn)?;
    Ok(())
}

pub fn insert_many_file_tags<C>(conn: &mut C, file_tags: Vec<FileTag>) -> Result<(), Error>
where
    C: DaoConnection,
{
    for file_tag in file_tags {
        insert_file_tag(conn, file_tag)?;
    }
    Ok(())
}

pub fn upsert_many_file_tags<C>(conn: &mut C, file_tags: Vec<FileTag>) -> Result<(), Error>
where
    C: DaoConnection,
{
    for file_tag in file_tags {
        upsert_file_tag(conn, file_tag)?;
    }
    Ok(())
}

pub fn select_all_tags<C>(conn: &mut C) -> Result<Vec<String>, Error>
where
    C: DaoConnection,
{
    let tags = file_tags::table
        .select(file_tags::columns::tag)
        .distinct()
        .load(conn)?;
    Ok(tags)
}

pub fn select_all_file_tags<C>(conn: &mut C) -> Result<Vec<FileTag>, Error>
where
    C: DaoConnection,
{
    let file_tags = file_tags::table.load(conn)?;
    Ok(file_tags)
}

pub fn select_file_tags_by_file_id<C>(conn: &mut C, file_id: i32) -> Result<Vec<FileTag>, Error>
where
    C: DaoConnection,
{
    let file_tags = file_tags::table
        .filter(file_tags::file_id.eq(file_id))
        .select(FileTag::as_select())
        .load(conn)?;
    Ok(file_tags)
}

pub fn select_file_tags_by_tag<C>(conn: &mut C, tag: &str) -> Result<Vec<FileTag>, Error>
where
    C: DaoConnection,
{
    let file_tags = file_tags::table
        .filter(file_tags::tag.eq(tag))
        .select(FileTag::as_select())
        .load(conn)?;
    Ok(file_tags)
}

pub fn delete_file_tag<C>(conn: &mut C, file_tag: FileTag) -> Result<(), Error>
where
    C: DaoConnection,
{
    tracing::debug!("deleting tag: {file_tag:?}");
    diesel::delete(
        file_tags::table.filter(
            file_tags::file_id
                .eq(file_tag.file_id)
                .and(file_tags::tag.eq(file_tag.tag)),
        ),
    )
    .execute(conn)?;
    Ok(())
}

pub fn insert_directory<C>(conn: &mut C, directory: NewDirectory) -> Result<Directory, Error>
where
    C: DaoConnection,
{
    let result = diesel::insert_into(directories::table)
        .values(&directory)
        .returning(Directory::as_returning())
        .get_result(conn)?;
    Ok(result)
}

pub fn upsert_directory<C>(conn: &mut C, directory: NewDirectory) -> Result<(), Error>
where
    C: DaoConnection,
{
    diesel::insert_into(directories::table)
        .values(&directory)
        .on_conflict_do_nothing()
        .execute(conn)?;
    Ok(())
}

pub fn insert_many_directories<C>(conn: &mut C, directories: Vec<NewDirectory>) -> Result<(), Error>
where
    C: DaoConnection,
{
    for directory in directories {
        insert_directory(conn, directory)?;
    }
    Ok(())
}

pub fn upsert_many_directories<C>(conn: &mut C, directories: Vec<NewDirectory>) -> Result<(), Error>
where
    C: DaoConnection,
{
    for directory in directories {
        upsert_directory(conn, directory)?;
    }
    Ok(())
}

pub fn insert_remote<C>(conn: &mut C, remote: NewRemote) -> Result<Remote, Error>
where
    C: DaoConnection,
{
    let result = diesel::insert_into(remotes::table)
        .values(&remote)
        .returning(Remote::as_returning())
        .get_result(conn)?;
    Ok(result)
}

pub fn select_all_remotes<C>(conn: &mut C) -> Result<Vec<Remote>, Error>
where
    C: DaoConnection,
{
    let remotes = remotes::table
        .order_by(remotes::columns::order)
        .load(conn)?;
    Ok(remotes)
}

pub fn delete_remote_by_id<C>(conn: &mut C, id: i32) -> Result<(), Error>
where
    C: DaoConnection,
{
    conn.transaction(|conn| {
        diesel::delete(remotes::table.filter(remotes::id.eq(id))).execute(conn)?;

        // ensure that there are no gaps in the `order`
        sql_query(
            r#"
            UPDATE remotes
            SET "order" = updated_values.new_order - 1
            FROM (SELECT rowid, ROW_NUMBER() OVER (ORDER BY "order") AS new_order FROM remotes) AS updated_values
            WHERE remotes.rowid = updated_values.rowid
        "#,
        )
        .execute(conn)?;

        Ok(())
    })
}

pub fn swap_order_of_remotes<C>(conn: &mut C, a: &Remote, b: &Remote) -> Result<(), Error>
where
    C: DaoConnection,
{
    conn.transaction(|conn| {
        sql_query(
            r#"
            UPDATE remotes SET "order" = CASE id
                WHEN ? THEN ?
                WHEN ? THEN ?
            END
            WHERE id IN (?, ?)
        "#,
        )
        .bind::<Integer, _>(a.id)
        .bind::<Integer, _>(-b.order - 1)
        .bind::<Integer, _>(b.id)
        .bind::<Integer, _>(-a.order - 1)
        .bind::<Integer, _>(a.id)
        .bind::<Integer, _>(b.id)
        .execute(conn)?;

        sql_query(
            r#"
            UPDATE remotes SET "order" = CASE id
                WHEN ? THEN ?
                WHEN ? THEN ?
            END
            WHERE id IN (?, ?)
        "#,
        )
        .bind::<Integer, _>(a.id)
        .bind::<Integer, _>(b.order)
        .bind::<Integer, _>(b.id)
        .bind::<Integer, _>(a.order)
        .bind::<Integer, _>(a.id)
        .bind::<Integer, _>(b.id)
        .execute(conn)?;

        Ok(())
    })
}

pub fn get_reading_progress<C>(
    conn: &mut C,
    fingerprint: &str,
) -> Result<Option<ReadingProgress>, Error>
where
    C: DaoConnection,
{
    let result = reading_progress::table
        .find(fingerprint)
        .select(ReadingProgress::as_select())
        .first(conn)
        .optional()?;
    Ok(result)
}

pub fn upsert_reading_progress<C>(conn: &mut C, progress: ReadingProgress) -> Result<(), Error>
where
    C: DaoConnection,
{
    sql_query(
        r#"
        INSERT INTO reading_progress (fingerprint, progress, last_updated)
        VALUES (?, ?, ?)
        ON CONFLICT(fingerprint) DO UPDATE
        SET progress = excluded.progress,
            last_updated = excluded.last_updated
        WHERE excluded.last_updated > reading_progress.last_updated
        "#,
    )
    .bind::<Text, _>(&progress.fingerprint)
    .bind::<Text, _>(&progress.progress)
    .bind::<Text, _>(&progress.last_updated)
    .execute(conn)?;
    Ok(())
}
