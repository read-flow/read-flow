// SPDX-License-Identifier: AGPL-3.0-or-later

//! File-row queries (`files` table) and the high-level scan writer.

use sqlx::SqliteConnection;

use super::Error;
use super::tags::upsert_content;
use super::tags::upsert_content_tag;
use crate::db::models::ContentTag;
use crate::db::models::File;
use crate::db::models::NewFile;

/// Shared JOIN fragment used by all file SELECT queries.
/// Status is derived from reading_state (defaults to 0/Unread when no row exists).
pub(super) const FILE_SELECT: &str = r#"
    SELECT f.id, f.guid, f.path, f.type, f.size, f.fingerprint,
           f.archive_path, f.archive_inner_path, f.imported_at,
           COALESCE(rs.status, 0) AS status,
           d.guid AS document_guid
    FROM files f
    JOIN contents c ON f.fingerprint = c.fingerprint
    LEFT JOIN reading_state rs ON c.fingerprint = rs.fingerprint
    LEFT JOIN documents d ON c.document_id = d.id"#;

pub async fn insert_file(conn: &mut SqliteConnection, file: NewFile) -> Result<File, Error> {
    sqlx::query(
        r#"INSERT INTO files (guid, path, "type", size, fingerprint, archive_path, archive_inner_path, imported_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))"#,
    )
    .bind(&file.guid)
    .bind(&file.path)
    .bind(&file.type_)
    .bind(file.size)
    .bind(&file.fingerprint)
    .bind(&file.archive_path)
    .bind(&file.archive_inner_path)
    .execute(&mut *conn)
    .await?;
    // NB: the row is read back via `FILE_SELECT`, which requires the matching
    // `contents` row — callers must upsert it first (see `write_scanned_file`).
    let row = select_file_by_path(&mut *conn, &file.path)
        .await?
        .ok_or_else(|| Error::Sqlx(std::sync::Arc::new(sqlx::Error::RowNotFound)))?;
    Ok(row)
}

pub async fn upsert_file(conn: &mut SqliteConnection, file: NewFile) -> Result<(), Error> {
    sqlx::query(
        r#"INSERT OR IGNORE INTO files (guid, path, "type", size, fingerprint, archive_path, archive_inner_path, imported_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))"#,
    )
    .bind(&file.guid)
    .bind(&file.path)
    .bind(&file.type_)
    .bind(file.size)
    .bind(&file.fingerprint)
    .bind(&file.archive_path)
    .bind(&file.archive_inner_path)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn update_file(conn: &mut SqliteConnection, file: &File) -> Result<(), Error> {
    sqlx::query(r#"UPDATE files SET path = ?, "type" = ?, size = ?, fingerprint = ? WHERE id = ?"#)
        .bind(&file.path)
        .bind(&file.type_)
        .bind(file.size)
        .bind(&file.fingerprint)
        .bind(file.id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn select_all_files(conn: &mut SqliteConnection) -> Result<Vec<File>, Error> {
    sqlx::query_as::<_, File>(FILE_SELECT)
        .fetch_all(&mut *conn)
        .await
        .map_err(Into::into)
}
pub async fn select_file_by_id(
    conn: &mut SqliteConnection,
    id: i32,
) -> Result<Option<File>, Error> {
    sqlx::query_as::<_, File>(sqlx::AssertSqlSafe(format!("{FILE_SELECT} WHERE f.id = ?")))
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(Into::into)
}

pub async fn select_file_by_guid(
    conn: &mut SqliteConnection,
    guid: &str,
) -> Result<Option<File>, Error> {
    sqlx::query_as::<_, File>(sqlx::AssertSqlSafe(format!(
        "{FILE_SELECT} WHERE f.guid = ?"
    )))
    .bind(guid)
    .fetch_optional(&mut *conn)
    .await
    .map_err(Into::into)
}

pub async fn select_file_by_path(
    conn: &mut SqliteConnection,
    path: &str,
) -> Result<Option<File>, Error> {
    sqlx::query_as::<_, File>(sqlx::AssertSqlSafe(format!(
        "{FILE_SELECT} WHERE f.path = ?"
    )))
    .bind(path)
    .fetch_optional(&mut *conn)
    .await
    .map_err(Into::into)
}

pub async fn select_all_files_by_path_like(
    conn: &mut SqliteConnection,
    path: &str,
) -> Result<Vec<File>, Error> {
    sqlx::query_as::<_, File>(sqlx::AssertSqlSafe(format!(
        "{FILE_SELECT} WHERE f.path LIKE ?"
    )))
    .bind(path)
    .fetch_all(&mut *conn)
    .await
    .map_err(Into::into)
}

pub async fn delete_file_record(conn: &mut SqliteConnection, id: i32) -> Result<(), Error> {
    sqlx::query("DELETE FROM files WHERE id = ?")
        .bind(id)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

// ─── High-level scan writer ───────────────────────────────────────────────────

/// Write a single scanned file (upsert content + upsert file + add tags).
/// For archive members, `archive` is `(archive_path, inner_path)` and `path`
/// is the synthetic unique form `"{archive_path}::{inner_path}"`.
/// Returns `(was_new, was_updated)`.
pub async fn write_scanned_file(
    conn: &mut SqliteConnection,
    path: &str,
    extension: &str,
    size: i64,
    fingerprint: &str,
    tags: &[String],
    archive: Option<(&str, &str)>,
) -> Result<(bool, bool), Error> {
    // Ensure content row exists for this fingerprint.
    upsert_content(&mut *conn, fingerprint).await?;

    let (was_new, was_updated) = match select_file_by_path(&mut *conn, path).await? {
        None => {
            let guid = uuid::Uuid::new_v4().to_string();
            insert_file(
                &mut *conn,
                NewFile {
                    guid,
                    path: path.to_owned(),
                    type_: extension.to_owned(),
                    size: size as i32,
                    fingerprint: fingerprint.to_owned(),
                    archive_path: archive.map(|(a, _)| a.to_owned()),
                    archive_inner_path: archive.map(|(_, i)| i.to_owned()),
                },
            )
            .await?;
            (true, false)
        }
        Some(existing) => {
            let changed = existing.size as i64 != size || existing.fingerprint != fingerprint;
            if changed {
                // Ensure content row exists for the new fingerprint before updating the FK.
                upsert_content(&mut *conn, fingerprint).await?;
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
            (false, changed)
        }
    };

    for tag in tags {
        upsert_content_tag(
            &mut *conn,
            ContentTag::new(fingerprint.to_owned(), tag.clone()),
        )
        .await?;
    }

    Ok((was_new, was_updated))
}
