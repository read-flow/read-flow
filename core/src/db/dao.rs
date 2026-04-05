use std::io;
use std::sync::Arc;

use sqlx::SqliteConnection;
use sqlx::SqlitePool;

use crate::db::models::File;
use crate::db::models::FileTag;
use crate::db::models::NewFile;
use crate::db::models::NewRemote;
use crate::db::models::ReadingProgress;
use crate::db::models::Remote;

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Sqlx(#[source] Arc<sqlx::Error>),
    #[error("io error: {0}")]
    IO(#[source] Arc<io::Error>),
}

impl From<sqlx::Error> for Error {
    fn from(value: sqlx::Error) -> Self {
        Self::Sqlx(Arc::new(value))
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::IO(Arc::new(value))
    }
}

pub async fn insert_file(conn: &mut SqliteConnection, file: NewFile) -> Result<File, Error> {
    let row = sqlx::query_as::<_, File>(
        r#"INSERT INTO files (path, "type", size, fingerprint, status)
         VALUES (?, ?, ?, ?, ?)
         RETURNING id, path, "type", size, fingerprint, status"#,
    )
    .bind(&file.path)
    .bind(&file.type_)
    .bind(file.size)
    .bind(&file.fingerprint)
    .bind(file.status)
    .fetch_one(&mut *conn)
    .await?;
    Ok(row)
}

pub async fn upsert_file(conn: &mut SqliteConnection, file: NewFile) -> Result<(), Error> {
    sqlx::query(
        r#"INSERT OR IGNORE INTO files (path, "type", size, fingerprint, status) VALUES (?, ?, ?, ?, ?)"#,
    )
    .bind(&file.path)
    .bind(&file.type_)
    .bind(file.size)
    .bind(&file.fingerprint)
    .bind(file.status)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn update_file(conn: &mut SqliteConnection, file: File) -> Result<(), Error> {
    sqlx::query(
        r#"UPDATE files SET path = ?, "type" = ?, size = ?, fingerprint = ?, status = ? WHERE id = ?"#,
    )
    .bind(&file.path)
    .bind(&file.type_)
    .bind(file.size)
    .bind(&file.fingerprint)
    .bind(file.status)
    .bind(file.id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn select_all_files(conn: &mut SqliteConnection) -> Result<Vec<File>, Error> {
    let files =
        sqlx::query_as::<_, File>("SELECT id, path, type, size, fingerprint, status FROM files")
            .fetch_all(&mut *conn)
            .await?;
    Ok(files)
}

pub async fn select_all_files_order_by_id(conn: &mut SqliteConnection) -> Result<Vec<File>, Error> {
    let files = sqlx::query_as::<_, File>(
        "SELECT id, path, type, size, fingerprint, status FROM files ORDER BY id",
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(files)
}

pub async fn select_all_files_order_by_type(
    conn: &mut SqliteConnection,
) -> Result<Vec<File>, Error> {
    let files = sqlx::query_as::<_, File>(
        "SELECT id, path, type, size, fingerprint, status FROM files ORDER BY type",
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(files)
}

pub async fn select_all_files_order_by_path(
    conn: &mut SqliteConnection,
) -> Result<Vec<File>, Error> {
    let files = sqlx::query_as::<_, File>(
        "SELECT id, path, type, size, fingerprint, status FROM files ORDER BY path",
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(files)
}

pub async fn select_all_files_order_by_size(
    conn: &mut SqliteConnection,
) -> Result<Vec<File>, Error> {
    let files = sqlx::query_as::<_, File>(
        "SELECT id, path, type, size, fingerprint, status FROM files ORDER BY size",
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(files)
}

pub async fn select_all_files_order_by_fingerprint(
    conn: &mut SqliteConnection,
) -> Result<Vec<File>, Error> {
    let files = sqlx::query_as::<_, File>(
        "SELECT id, path, type, size, fingerprint, status FROM files ORDER BY fingerprint",
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(files)
}

pub async fn select_file_by_id(
    conn: &mut SqliteConnection,
    id: i32,
) -> Result<Option<File>, Error> {
    let file = sqlx::query_as::<_, File>(
        "SELECT id, path, type, size, fingerprint, status FROM files WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(file)
}

pub async fn select_file_by_path(
    conn: &mut SqliteConnection,
    path: &str,
) -> Result<Option<File>, Error> {
    let file = sqlx::query_as::<_, File>(
        "SELECT id, path, type, size, fingerprint, status FROM files WHERE path = ?",
    )
    .bind(path)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(file)
}

pub async fn select_all_files_by_path_like(
    conn: &mut SqliteConnection,
    path: &str,
) -> Result<Vec<File>, Error> {
    let files = sqlx::query_as::<_, File>(
        "SELECT id, path, type, size, fingerprint, status FROM files WHERE path LIKE ?",
    )
    .bind(path)
    .fetch_all(&mut *conn)
    .await?;
    Ok(files)
}

pub async fn delete_file_record(pool: &SqlitePool, id: i32) -> Result<(), Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM file_tags WHERE file_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM files WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn insert_file_tag(
    conn: &mut SqliteConnection,
    file_tag: FileTag,
) -> Result<FileTag, Error> {
    tracing::debug!("inserting tag: {file_tag:?}");
    let row = sqlx::query_as::<_, FileTag>(
        "INSERT INTO file_tags (file_id, tag) VALUES (?, ?) RETURNING file_id, tag",
    )
    .bind(file_tag.file_id)
    .bind(&file_tag.tag)
    .fetch_one(&mut *conn)
    .await?;
    Ok(row)
}

pub async fn upsert_file_tag(conn: &mut SqliteConnection, file_tag: FileTag) -> Result<(), Error> {
    tracing::debug!("upserting tag: {file_tag:?}");
    sqlx::query("INSERT OR IGNORE INTO file_tags (file_id, tag) VALUES (?, ?)")
        .bind(file_tag.file_id)
        .bind(&file_tag.tag)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn upsert_many_file_tags(
    conn: &mut SqliteConnection,
    file_tags: Vec<FileTag>,
) -> Result<(), Error> {
    for file_tag in file_tags {
        upsert_file_tag(&mut *conn, file_tag).await?;
    }
    Ok(())
}

pub async fn delete_file_tags(
    conn: &mut SqliteConnection,
    file_id: i32,
    tags: Vec<String>,
) -> Result<(), Error> {
    for tag in tags {
        sqlx::query("DELETE FROM file_tags WHERE file_id = ? AND tag = ?")
            .bind(file_id)
            .bind(&tag)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

pub async fn select_all_tags(conn: &mut SqliteConnection) -> Result<Vec<String>, Error> {
    let tags = sqlx::query_scalar::<_, String>("SELECT DISTINCT tag FROM file_tags ORDER BY tag")
        .fetch_all(&mut *conn)
        .await?;
    Ok(tags)
}

pub async fn select_all_file_tags(conn: &mut SqliteConnection) -> Result<Vec<FileTag>, Error> {
    let file_tags = sqlx::query_as::<_, FileTag>("SELECT file_id, tag FROM file_tags")
        .fetch_all(&mut *conn)
        .await?;
    Ok(file_tags)
}

pub async fn select_file_tags_by_file_id(
    conn: &mut SqliteConnection,
    file_id: i32,
) -> Result<Vec<FileTag>, Error> {
    let file_tags =
        sqlx::query_as::<_, FileTag>("SELECT file_id, tag FROM file_tags WHERE file_id = ?")
            .bind(file_id)
            .fetch_all(&mut *conn)
            .await?;
    Ok(file_tags)
}

pub async fn select_file_tags_by_tag(
    conn: &mut SqliteConnection,
    tag: &str,
) -> Result<Vec<FileTag>, Error> {
    let file_tags =
        sqlx::query_as::<_, FileTag>("SELECT file_id, tag FROM file_tags WHERE tag = ?")
            .bind(tag)
            .fetch_all(&mut *conn)
            .await?;
    Ok(file_tags)
}

pub async fn delete_file_tag(conn: &mut SqliteConnection, file_tag: FileTag) -> Result<(), Error> {
    tracing::debug!("deleting tag: {file_tag:?}");
    sqlx::query("DELETE FROM file_tags WHERE file_id = ? AND tag = ?")
        .bind(file_tag.file_id)
        .bind(&file_tag.tag)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn insert_remote(
    conn: &mut SqliteConnection,
    remote: NewRemote,
) -> Result<Remote, Error> {
    let row = sqlx::query_as::<_, Remote>(
        r#"INSERT INTO remotes (base_url, "order", passphrase, user_id)
           VALUES (?, ?, ?, ?)
           RETURNING id, base_url, "order" AS "order", passphrase, user_id"#,
    )
    .bind(&remote.base_url)
    .bind(remote.order)
    .bind(&remote.passphrase)
    .bind(&remote.user_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(row)
}

pub async fn select_all_remotes(conn: &mut SqliteConnection) -> Result<Vec<Remote>, Error> {
    let remotes = sqlx::query_as::<_, Remote>(
        r#"SELECT id, base_url, "order", passphrase, user_id FROM remotes ORDER BY "order""#,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(remotes)
}

pub async fn delete_remote_by_id(pool: &SqlitePool, id: i32) -> Result<(), Error> {
    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM remotes WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    // Ensure no gaps in the `order` column
    sqlx::query(
        r#"UPDATE remotes
           SET "order" = updated_values.new_order - 1
           FROM (SELECT rowid, ROW_NUMBER() OVER (ORDER BY "order") AS new_order FROM remotes) AS updated_values
           WHERE remotes.rowid = updated_values.rowid"#,
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

pub async fn swap_order_of_remotes(pool: &SqlitePool, a: &Remote, b: &Remote) -> Result<(), Error> {
    let mut tx = pool.begin().await?;

    // Move to temporary negative values to avoid UNIQUE constraint on "order"
    sqlx::query(
        r#"UPDATE remotes SET "order" = CASE id
               WHEN ? THEN ?
               WHEN ? THEN ?
           END
           WHERE id IN (?, ?)"#,
    )
    .bind(a.id)
    .bind(-b.order - 1)
    .bind(b.id)
    .bind(-a.order - 1)
    .bind(a.id)
    .bind(b.id)
    .execute(&mut *tx)
    .await?;

    // Set the final swapped values
    sqlx::query(
        r#"UPDATE remotes SET "order" = CASE id
               WHEN ? THEN ?
               WHEN ? THEN ?
           END
           WHERE id IN (?, ?)"#,
    )
    .bind(a.id)
    .bind(b.order)
    .bind(b.id)
    .bind(a.order)
    .bind(a.id)
    .bind(b.id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

pub async fn get_reading_progress(
    conn: &mut SqliteConnection,
    fingerprint: &str,
) -> Result<Option<ReadingProgress>, Error> {
    let result = sqlx::query_as::<_, ReadingProgress>(
        "SELECT fingerprint, progress, last_updated FROM reading_progress WHERE fingerprint = ?",
    )
    .bind(fingerprint)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(result)
}

pub async fn upsert_reading_progress(
    conn: &mut SqliteConnection,
    progress: ReadingProgress,
) -> Result<(), Error> {
    sqlx::query(
        r#"INSERT INTO reading_progress (fingerprint, progress, last_updated)
           VALUES (?, ?, ?)
           ON CONFLICT(fingerprint) DO UPDATE
           SET progress = excluded.progress,
               last_updated = excluded.last_updated
           WHERE excluded.last_updated > reading_progress.last_updated"#,
    )
    .bind(&progress.fingerprint)
    .bind(&progress.progress)
    .bind(&progress.last_updated)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Write a single scanned file (upsert + add tags). Returns `(was_new, was_updated)`.
pub async fn write_scanned_file(
    conn: &mut SqliteConnection,
    path: &str,
    extension: &str,
    size: i64,
    fingerprint: &str,
    tags: &[String],
) -> Result<(bool, bool), Error> {
    let (file_id, was_new, was_updated) = match select_file_by_path(&mut *conn, path).await? {
        None => {
            let file = insert_file(
                &mut *conn,
                NewFile {
                    path: path.to_owned(),
                    type_: extension.to_owned(),
                    size: size as i32,
                    fingerprint: fingerprint.to_owned(),
                    status: 0,
                },
            )
            .await?;
            (file.id, true, false)
        }
        Some(existing) => {
            let changed = existing.size as i64 != size || existing.fingerprint != fingerprint;
            if changed {
                sqlx::query("UPDATE files SET size = ?, fingerprint = ? WHERE id = ?")
                    .bind(size)
                    .bind(fingerprint)
                    .bind(existing.id)
                    .execute(&mut *conn)
                    .await?;
                tracing::info!(
                    "updated file: {} (size: {} → {}, fingerprint: {} → {})",
                    path,
                    existing.size,
                    size,
                    existing.fingerprint,
                    fingerprint
                );
            }
            (existing.id, false, changed)
        }
    };

    for tag in tags {
        upsert_file_tag(&mut *conn, FileTag::new(file_id, tag.clone())).await?;
    }

    Ok((was_new, was_updated))
}
