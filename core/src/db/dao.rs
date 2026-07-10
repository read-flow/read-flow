// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::Arc;

use sqlx::SqliteConnection;
use sqlx::SqlitePool;

use crate::api::ApiDocument;
use crate::api::DocumentMeta;
use crate::db::models::ContentTag;
use crate::db::models::Document;
use crate::db::models::DocumentUserMetadata;
use crate::db::models::File;
use crate::db::models::NewFile;
use crate::db::models::NewRemote;
use crate::db::models::ReadingState;
use crate::db::models::Remote;
use crate::scan::metadata::ExtractedMetadata;

// ─── Error ───────────────────────────────────────────────────────────────────

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

// ─── File queries ─────────────────────────────────────────────────────────────

/// Shared JOIN fragment used by all file SELECT queries.
/// Status is derived from reading_state (defaults to 0/Unread when no row exists).
const FILE_SELECT: &str = r#"
    SELECT f.id, f.guid, f.path, f.type, f.size, f.fingerprint,
           f.archive_path, f.archive_inner_path,
           COALESCE(rs.status, 0) AS status,
           d.guid AS document_guid
    FROM files f
    JOIN contents c ON f.fingerprint = c.fingerprint
    LEFT JOIN reading_state rs ON c.fingerprint = rs.fingerprint
    LEFT JOIN documents d ON c.document_id = d.id"#;

pub async fn insert_file(conn: &mut SqliteConnection, file: NewFile) -> Result<File, Error> {
    sqlx::query(
        r#"INSERT INTO files (guid, path, "type", size, fingerprint, archive_path, archive_inner_path)
           VALUES (?, ?, ?, ?, ?, ?, ?)"#,
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
    let row = select_file_by_path(&mut *conn, &file.path)
        .await?
        .expect("file must exist after insert");
    Ok(row)
}

pub async fn upsert_file(conn: &mut SqliteConnection, file: NewFile) -> Result<(), Error> {
    sqlx::query(
        r#"INSERT OR IGNORE INTO files (guid, path, "type", size, fingerprint, archive_path, archive_inner_path)
           VALUES (?, ?, ?, ?, ?, ?, ?)"#,
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

pub async fn select_all_files_order_by_id(conn: &mut SqliteConnection) -> Result<Vec<File>, Error> {
    sqlx::query_as::<_, File>(sqlx::AssertSqlSafe(format!("{FILE_SELECT} ORDER BY f.id")))
        .fetch_all(&mut *conn)
        .await
        .map_err(Into::into)
}

pub async fn select_all_files_order_by_type(
    conn: &mut SqliteConnection,
) -> Result<Vec<File>, Error> {
    sqlx::query_as::<_, File>(sqlx::AssertSqlSafe(format!(
        "{FILE_SELECT} ORDER BY f.type"
    )))
    .fetch_all(&mut *conn)
    .await
    .map_err(Into::into)
}

pub async fn select_all_files_order_by_path(
    conn: &mut SqliteConnection,
) -> Result<Vec<File>, Error> {
    sqlx::query_as::<_, File>(sqlx::AssertSqlSafe(format!(
        "{FILE_SELECT} ORDER BY f.path"
    )))
    .fetch_all(&mut *conn)
    .await
    .map_err(Into::into)
}

pub async fn select_all_files_order_by_size(
    conn: &mut SqliteConnection,
) -> Result<Vec<File>, Error> {
    sqlx::query_as::<_, File>(sqlx::AssertSqlSafe(format!(
        "{FILE_SELECT} ORDER BY f.size"
    )))
    .fetch_all(&mut *conn)
    .await
    .map_err(Into::into)
}

pub async fn select_all_files_order_by_fingerprint(
    conn: &mut SqliteConnection,
) -> Result<Vec<File>, Error> {
    sqlx::query_as::<_, File>(sqlx::AssertSqlSafe(format!(
        "{FILE_SELECT} ORDER BY f.fingerprint"
    )))
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

pub async fn delete_file_record(pool: &SqlitePool, id: i32) -> Result<(), Error> {
    sqlx::query("DELETE FROM files WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ─── Content queries ──────────────────────────────────────────────────────────

pub async fn upsert_content(conn: &mut SqliteConnection, fingerprint: &str) -> Result<(), Error> {
    sqlx::query("INSERT OR IGNORE INTO contents (fingerprint) VALUES (?)")
        .bind(fingerprint)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

// ─── Content tag queries ──────────────────────────────────────────────────────

pub async fn upsert_content_tag(conn: &mut SqliteConnection, tag: ContentTag) -> Result<(), Error> {
    tracing::debug!("upserting content tag: {tag:?}");
    sqlx::query("INSERT OR IGNORE INTO content_tags (fingerprint, tag) VALUES (?, ?)")
        .bind(&tag.fingerprint)
        .bind(&tag.tag)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn upsert_many_content_tags(
    conn: &mut SqliteConnection,
    tags: Vec<ContentTag>,
) -> Result<(), Error> {
    for tag in tags {
        upsert_content_tag(&mut *conn, tag).await?;
    }
    Ok(())
}

pub async fn delete_content_tags(
    conn: &mut SqliteConnection,
    fingerprint: &str,
    tags: Vec<String>,
) -> Result<(), Error> {
    for tag in tags {
        sqlx::query("DELETE FROM content_tags WHERE fingerprint = ? AND tag = ?")
            .bind(fingerprint)
            .bind(&tag)
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

pub async fn select_all_content_tags(
    conn: &mut SqliteConnection,
) -> Result<Vec<ContentTag>, Error> {
    sqlx::query_as::<_, ContentTag>("SELECT fingerprint, tag FROM content_tags")
        .fetch_all(&mut *conn)
        .await
        .map_err(Into::into)
}

pub async fn select_content_tags_by_fingerprint(
    conn: &mut SqliteConnection,
    fingerprint: &str,
) -> Result<Vec<ContentTag>, Error> {
    sqlx::query_as::<_, ContentTag>(
        "SELECT fingerprint, tag FROM content_tags WHERE fingerprint = ?",
    )
    .bind(fingerprint)
    .fetch_all(&mut *conn)
    .await
    .map_err(Into::into)
}

pub async fn select_all_distinct_tags(conn: &mut SqliteConnection) -> Result<Vec<String>, Error> {
    sqlx::query_scalar::<_, String>("SELECT DISTINCT tag FROM content_tags ORDER BY tag")
        .fetch_all(&mut *conn)
        .await
        .map_err(Into::into)
}

pub async fn select_all_files_excluding_tags(
    conn: &mut SqliteConnection,
    excluded: &[String],
) -> Result<Vec<File>, Error> {
    if excluded.is_empty() {
        return select_all_files(conn).await;
    }
    let placeholders = excluded.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let query = format!(
        "{FILE_SELECT} WHERE NOT EXISTS (
            SELECT 1 FROM content_tags ct
            WHERE ct.fingerprint = f.fingerprint
            AND ct.tag IN ({placeholders})
        )"
    );
    let mut q = sqlx::query_as::<_, File>(sqlx::AssertSqlSafe(query));
    for tag in excluded {
        q = q.bind(tag);
    }
    q.fetch_all(&mut *conn).await.map_err(Into::into)
}

pub async fn select_all_distinct_tags_excluding(
    conn: &mut SqliteConnection,
    excluded: &[String],
) -> Result<Vec<String>, Error> {
    if excluded.is_empty() {
        return select_all_distinct_tags(conn).await;
    }
    let placeholders = excluded.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let query = format!(
        "SELECT DISTINCT tag FROM content_tags WHERE tag NOT IN ({placeholders}) ORDER BY tag"
    );
    let mut q = sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(query));
    for tag in excluded {
        q = q.bind(tag);
    }
    q.fetch_all(&mut *conn).await.map_err(Into::into)
}

// ─── Document queries ────────────────────────────────────────────────────────

/// Insert a document with `guid` if it doesn't already exist and return it.
pub async fn upsert_document(conn: &mut SqliteConnection, guid: &str) -> Result<Document, Error> {
    sqlx::query("INSERT OR IGNORE INTO documents (guid) VALUES (?)")
        .bind(guid)
        .execute(&mut *conn)
        .await?;
    let doc = sqlx::query_as::<_, Document>("SELECT id, guid FROM documents WHERE guid = ?")
        .bind(guid)
        .fetch_one(&mut *conn)
        .await?;
    Ok(doc)
}

/// Set `document_id` on a content row, but only when it is currently NULL.
/// This preserves any existing link (whether user-set or from a prior auto-pass).
pub async fn set_content_document(
    conn: &mut SqliteConnection,
    fingerprint: &str,
    document_id: i32,
) -> Result<(), Error> {
    sqlx::query(
        "UPDATE contents SET document_id = ? WHERE fingerprint = ? AND document_id IS NULL",
    )
    .bind(document_id)
    .bind(fingerprint)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Post-scan pass: group all known files by `(parent_directory, stem)` and link
/// contents that share a stem but have distinct fingerprints to a common `Document`.
///
/// When multiple documents already exist in a group they are merged: metadata from
/// non-canonical documents is merged into the canonical one (extending the authors
/// list), and all content rows are pointed at the canonical document.
pub async fn auto_link_documents(pool: &SqlitePool) -> Result<(), Error> {
    #[derive(sqlx::FromRow)]
    struct FileForLinking {
        path: String,
        fingerprint: String,
        document_id: Option<i32>,
    }

    let mut conn = pool.acquire().await?;

    let rows = sqlx::query_as::<_, FileForLinking>(
        "SELECT f.path, f.fingerprint, c.document_id
         FROM files f JOIN contents c ON f.fingerprint = c.fingerprint",
    )
    .fetch_all(&mut *conn)
    .await?;

    // Group by (parent_dir, stem) — both case-sensitive strings.
    let mut groups: HashMap<(String, String), Vec<FileForLinking>> = HashMap::new();
    for row in rows {
        let path = Path::new(&row.path);
        let parent = path
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        groups.entry((parent, stem)).or_default().push(row);
    }

    for files in groups.into_values() {
        // Only process groups with ≥ 2 distinct fingerprints.
        let distinct_fps: std::collections::HashSet<&str> =
            files.iter().map(|f| f.fingerprint.as_str()).collect();
        if distinct_fps.len() <= 1 {
            continue;
        }

        // Collect the distinct document_ids present in this group.
        let mut seen_ids = std::collections::HashSet::new();
        let distinct_doc_ids: Vec<i32> = files
            .iter()
            .filter_map(|f| f.document_id)
            .filter(|&id| seen_ids.insert(id))
            .collect();

        if distinct_doc_ids.len() == 1
            && files
                .iter()
                .all(|f| f.document_id == distinct_doc_ids.first().copied())
        {
            // Already fully linked to a single document — nothing to do.
            continue;
        }

        // Pick or create the canonical document.
        let canonical_id = if let Some(&first) = distinct_doc_ids.first() {
            first
        } else {
            let new_guid = uuid::Uuid::new_v4().to_string();
            let doc = upsert_document(&mut conn, &new_guid).await?;
            tracing::debug!(
                "created document {} for stem group ({} files)",
                doc.guid,
                files.len()
            );
            doc.id
        };

        // Merge metadata from every non-canonical document into the canonical one.
        for &other_id in distinct_doc_ids.iter().filter(|&&id| id != canonical_id) {
            merge_document_metadata_from_document(&mut conn, canonical_id, other_id).await?;
        }

        // Link all files in the group to the canonical document, overriding any
        // previously assigned document_id (removes the NULL-only restriction).
        for file in &files {
            sqlx::query("UPDATE contents SET document_id = ? WHERE fingerprint = ?")
                .bind(canonical_id)
                .bind(&file.fingerprint)
                .execute(&mut *conn)
                .await?;
        }
    }

    Ok(())
}

// ─── Remote queries ───────────────────────────────────────────────────────────

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

pub async fn update_remote(
    conn: &mut SqliteConnection,
    id: i32,
    base_url: &str,
    user_id: &str,
    passphrase: &str,
) -> Result<(), Error> {
    sqlx::query("UPDATE remotes SET base_url = ?, user_id = ?, passphrase = ? WHERE id = ?")
        .bind(base_url)
        .bind(user_id)
        .bind(passphrase)
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
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

// ─── Reading state queries ────────────────────────────────────────────────────

pub async fn get_reading_state(
    conn: &mut SqliteConnection,
    fingerprint: &str,
) -> Result<Option<ReadingState>, Error> {
    sqlx::query_as::<_, ReadingState>(
        "SELECT fingerprint, status, position, percentage, last_updated, status_updated_at \
         FROM reading_state WHERE fingerprint = ?",
    )
    .bind(fingerprint)
    .fetch_optional(&mut *conn)
    .await
    .map_err(Into::into)
}

/// Upsert reading state with server-side auto-transitions:
/// - Unread (0) + percentage > 0.01  → Reading (1)
/// - Reading (1) + percentage ≥ 0.99 → Read (2)
/// - Read (2): no auto-downgrade
///
/// Returns the resulting state (after any transitions).
pub async fn upsert_reading_state(
    conn: &mut SqliteConnection,
    state: ReadingState,
) -> Result<ReadingState, Error> {
    // For the INSERT (new row) path, compute initial status from percentage.
    let initial_status: i32 = if state.percentage > 0.01 { 1 } else { 0 };
    let initial_status_updated_at = if initial_status > 0 {
        state.last_updated.clone()
    } else {
        state.status_updated_at.clone()
    };

    sqlx::query(
        r#"INSERT INTO reading_state
               (fingerprint, status, position, percentage, last_updated, status_updated_at)
           VALUES (?, ?, ?, ?, ?, ?)
           ON CONFLICT(fingerprint) DO UPDATE
           SET position          = excluded.position,
               percentage        = excluded.percentage,
               last_updated      = excluded.last_updated,
               status            = CASE
                   WHEN reading_state.status = 0 AND excluded.percentage > 0.01  THEN 1
                   WHEN reading_state.status = 1 AND excluded.percentage >= 0.99 THEN 2
                   ELSE reading_state.status
               END,
               status_updated_at = CASE
                   WHEN (reading_state.status = 0 AND excluded.percentage > 0.01)
                     OR (reading_state.status = 1 AND excluded.percentage >= 0.99)
                     THEN excluded.last_updated
                   ELSE reading_state.status_updated_at
               END
           WHERE excluded.last_updated > reading_state.last_updated"#,
    )
    .bind(&state.fingerprint)
    .bind(initial_status)
    .bind(&state.position)
    .bind(state.percentage)
    .bind(&state.last_updated)
    .bind(&initial_status_updated_at)
    .execute(&mut *conn)
    .await?;

    get_reading_state(conn, &state.fingerprint)
        .await?
        .ok_or_else(|| Error::Sqlx(Arc::new(sqlx::Error::RowNotFound)))
}

/// Manually override the reading status. Bypasses auto-transition rules.
/// Creates a reading_state row if none exists.
pub async fn update_reading_status_only(
    conn: &mut SqliteConnection,
    fingerprint: &str,
    status: i32,
) -> Result<(), Error> {
    sqlx::query(
        r#"INSERT INTO reading_state (fingerprint, status, status_updated_at, last_updated)
           VALUES (?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
           ON CONFLICT(fingerprint) DO UPDATE
           SET status            = excluded.status,
               status_updated_at = excluded.status_updated_at"#,
    )
    .bind(fingerprint)
    .bind(status)
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

// ─── Document user-metadata queries ──────────────────────────────────────────

pub async fn get_document_user_metadata(
    conn: &mut SqliteConnection,
    document_id: i32,
) -> Result<Option<DocumentUserMetadata>, Error> {
    sqlx::query_as::<_, DocumentUserMetadata>(
        "SELECT document_id, document_type, title, subtitle, authors, description, \
                language, publisher, identifier, date, subject, updated_at, \
                selected_cover_fingerprint \
         FROM document_metadata WHERE document_id = ?",
    )
    .bind(document_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(Into::into)
}

#[allow(clippy::too_many_arguments)]
pub async fn upsert_document_user_metadata(
    conn: &mut SqliteConnection,
    document_id: i32,
    document_type: Option<&str>,
    title: Option<&str>,
    subtitle: Option<&str>,
    authors: Option<&str>,
    description: Option<&str>,
    language: Option<&str>,
    publisher: Option<&str>,
    identifier: Option<&str>,
    date: Option<&str>,
    subject: Option<&str>,
    selected_cover_fingerprint: Option<&str>,
) -> Result<DocumentUserMetadata, Error> {
    sqlx::query(
        "INSERT INTO document_metadata \
             (document_id, document_type, title, subtitle, authors, description, \
              language, publisher, identifier, date, subject, selected_cover_fingerprint) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(document_id) DO UPDATE SET \
             document_type                = excluded.document_type, \
             title                        = excluded.title, \
             subtitle                     = excluded.subtitle, \
             authors                      = excluded.authors, \
             description                  = excluded.description, \
             language                     = excluded.language, \
             publisher                    = excluded.publisher, \
             identifier                   = excluded.identifier, \
             date                         = excluded.date, \
             subject                      = excluded.subject, \
             selected_cover_fingerprint   = excluded.selected_cover_fingerprint, \
             updated_at                   = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
    )
    .bind(document_id)
    .bind(document_type)
    .bind(title)
    .bind(subtitle)
    .bind(authors)
    .bind(description)
    .bind(language)
    .bind(publisher)
    .bind(identifier)
    .bind(date)
    .bind(subject)
    .bind(selected_cover_fingerprint)
    .execute(&mut *conn)
    .await?;

    let row = get_document_user_metadata(&mut *conn, document_id)
        .await?
        .expect("row must exist after upsert");
    Ok(row)
}

/// Smart-merge extracted file metadata into a document's metadata row.
///
/// Rules:
/// - Scalar fields (title, language, publisher, identifier, date, subject): keep
///   existing value if non-null; fill in from extracted only when absent.
/// - Authors: extend the existing list with any new unique values from the
///   extracted metadata so the user can choose the best-formatted name.
pub async fn merge_document_metadata_from_extracted(
    conn: &mut SqliteConnection,
    document_id: i32,
    meta: &ExtractedMetadata,
) -> Result<(), Error> {
    let existing = get_document_user_metadata(&mut *conn, document_id).await?;

    let authors_json = |authors: &[String]| -> Option<String> {
        if authors.is_empty() {
            None
        } else {
            serde_json::to_string(authors).ok()
        }
    };

    match existing {
        None => {
            // No row yet — insert directly from extracted metadata.
            let authors = authors_json(&meta.authors);
            sqlx::query(
                "INSERT INTO document_metadata \
                 (document_id, title, subtitle, authors, description, language, publisher, \
                  identifier, date, subject) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(document_id)
            .bind(&meta.title)
            .bind(&meta.subtitle)
            .bind(authors.as_deref())
            .bind(&meta.description)
            .bind(&meta.language)
            .bind(&meta.publisher)
            .bind(&meta.identifier)
            .bind(&meta.date)
            .bind(&meta.subject)
            .execute(&mut *conn)
            .await?;
        }
        Some(existing) => {
            // Merge: for scalar fields keep existing if set, fill from extracted otherwise.
            let merged_title = existing.title.or_else(|| meta.title.clone());
            let merged_subtitle = existing.subtitle.or_else(|| meta.subtitle.clone());
            let merged_description = existing.description.or_else(|| meta.description.clone());
            let merged_language = existing.language.or_else(|| meta.language.clone());
            let merged_publisher = existing.publisher.or_else(|| meta.publisher.clone());
            let merged_identifier = existing.identifier.or_else(|| meta.identifier.clone());
            let merged_date = existing.date.or_else(|| meta.date.clone());
            let merged_subject = existing.subject.or_else(|| meta.subject.clone());

            // Authors: parse existing JSON array, append any new unique values.
            let mut all_authors: Vec<String> = existing
                .authors
                .as_deref()
                .and_then(|s| {
                    serde_json::from_str(s)
                        .inspect_err(|e| {
                            tracing::warn!("failed to parse existing authors JSON: {e}")
                        })
                        .ok()
                })
                .unwrap_or_default();
            for author in &meta.authors {
                if !all_authors.contains(author) {
                    all_authors.push(author.clone());
                }
            }
            let merged_authors = authors_json(&all_authors);

            sqlx::query(
                "UPDATE document_metadata SET \
                 title = ?, subtitle = ?, authors = ?, description = ?, language = ?, \
                 publisher = ?, identifier = ?, date = ?, subject = ?, \
                 updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') \
                 WHERE document_id = ?",
            )
            .bind(&merged_title)
            .bind(merged_subtitle.as_deref())
            .bind(merged_authors.as_deref())
            .bind(merged_description.as_deref())
            .bind(&merged_language)
            .bind(&merged_publisher)
            .bind(&merged_identifier)
            .bind(&merged_date)
            .bind(&merged_subject)
            .bind(document_id)
            .execute(&mut *conn)
            .await?;
        }
    }

    Ok(())
}

/// Merge the metadata of `source_id` into `canonical_id` using the same smart-merge
/// rules as `merge_document_metadata_from_extracted`.
pub async fn merge_document_metadata_from_document(
    conn: &mut SqliteConnection,
    canonical_id: i32,
    source_id: i32,
) -> Result<(), Error> {
    let Some(src) = get_document_user_metadata(&mut *conn, source_id).await? else {
        return Ok(());
    };
    let src_authors: Vec<String> = src
        .authors
        .as_deref()
        .and_then(|s| {
            serde_json::from_str(s)
                .inspect_err(|e| tracing::warn!("failed to parse source authors JSON: {e}"))
                .ok()
        })
        .unwrap_or_default();
    let extracted = ExtractedMetadata {
        title: src.title,
        subtitle: src.subtitle,
        authors: src_authors,
        description: src.description,
        language: src.language,
        publisher: src.publisher,
        identifier: src.identifier,
        date: src.date,
        subject: src.subject,
    };
    merge_document_metadata_from_extracted(&mut *conn, canonical_id, &extracted).await?;

    // Propagate selected_cover_fingerprint from loser to winner only when the
    // winner has none yet (same keep-existing rule as other scalar fields).
    if let Some(fp) = src.selected_cover_fingerprint {
        sqlx::query(
            "UPDATE document_metadata \
             SET selected_cover_fingerprint = ? \
             WHERE document_id = ? AND selected_cover_fingerprint IS NULL",
        )
        .bind(fp)
        .bind(canonical_id)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Merge `loser_guids` documents into `winner_guid`, then delete the losers.
///
/// For each loser:
/// 1. Re-assigns all `contents` rows from the loser's `document_id` to the winner's.
/// 2. Smart-merges the loser's metadata into the winner's (winner fields win on conflict).
/// 3. Deletes the loser `documents` row (CASCADE removes its `document_metadata` row).
///
/// Unknown GUIDs are silently skipped.
pub async fn merge_documents(
    pool: &SqlitePool,
    winner_guid: &str,
    loser_guids: &[String],
) -> Result<(), Error> {
    let mut tx = pool.begin().await?;

    let Some(winner_id) = sqlx::query_scalar::<_, i32>("SELECT id FROM documents WHERE guid = ?")
        .bind(winner_guid)
        .fetch_optional(&mut *tx)
        .await?
    else {
        return Ok(());
    };

    for loser_guid in loser_guids {
        if loser_guid == winner_guid {
            continue;
        }
        let Some(loser_id) =
            sqlx::query_scalar::<_, i32>("SELECT id FROM documents WHERE guid = ?")
                .bind(loser_guid)
                .fetch_optional(&mut *tx)
                .await?
        else {
            continue;
        };

        // Merge metadata from loser into winner before deleting the loser.
        merge_document_metadata_from_document(&mut tx, winner_id, loser_id).await?;

        // Reassign all contents from the loser to the winner.
        sqlx::query("UPDATE contents SET document_id = ? WHERE document_id = ?")
            .bind(winner_id)
            .bind(loser_id)
            .execute(&mut *tx)
            .await?;

        // Delete the loser document row (CASCADE removes its document_metadata row).
        sqlx::query("DELETE FROM documents WHERE id = ?")
            .bind(loser_id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(())
}

/// Get or create a `documents` row for the file identified by `file_guid`.
///
/// Get or create a `documents` row for the content identified by `fingerprint`.
pub async fn ensure_document_for_fingerprint(
    conn: &mut SqliteConnection,
    fingerprint: &str,
) -> Result<ApiDocument, Error> {
    let document_id: Option<i32> =
        sqlx::query_scalar("SELECT document_id FROM contents WHERE fingerprint = ?")
            .bind(fingerprint)
            .fetch_optional(&mut *conn)
            .await?
            .flatten();

    let (document_id, document_guid) = if let Some(id) = document_id {
        let guid: String = sqlx::query_scalar("SELECT guid FROM documents WHERE id = ?")
            .bind(id)
            .fetch_one(&mut *conn)
            .await?;
        (id, guid)
    } else {
        let new_guid = uuid::Uuid::new_v4().to_string();
        let doc = upsert_document(&mut *conn, &new_guid).await?;
        sqlx::query("UPDATE contents SET document_id = ? WHERE fingerprint = ?")
            .bind(doc.id)
            .bind(fingerprint)
            .execute(&mut *conn)
            .await?;
        (doc.id, new_guid)
    };

    load_api_document(&mut *conn, document_id, document_guid).await
}

/// Get or create a `documents` row for the file identified by `file_guid`.
pub async fn ensure_document_for_file_guid(
    conn: &mut SqliteConnection,
    file_guid: &str,
) -> Result<ApiDocument, Error> {
    let file = select_file_by_guid(&mut *conn, file_guid)
        .await?
        .ok_or_else(|| Error::Sqlx(Arc::new(sqlx::Error::RowNotFound)))?;
    ensure_document_for_fingerprint(&mut *conn, &file.fingerprint).await
}

// ─── High-level document queries (ApiDocument) ───────────────────────────────

pub async fn select_all_api_documents(
    conn: &mut SqliteConnection,
) -> Result<Vec<ApiDocument>, Error> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: i32,
        guid: String,
    }
    let rows = sqlx::query_as::<_, Row>("SELECT id, guid FROM documents")
        .fetch_all(&mut *conn)
        .await?;
    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        result.push(load_api_document(&mut *conn, row.id, row.guid).await?);
    }
    Ok(result)
}

pub async fn select_api_document_by_guid(
    conn: &mut SqliteConnection,
    guid: &str,
) -> Result<Option<ApiDocument>, Error> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: i32,
        guid: String,
    }
    let row = sqlx::query_as::<_, Row>("SELECT id, guid FROM documents WHERE guid = ?")
        .bind(guid)
        .fetch_optional(&mut *conn)
        .await?;
    match row {
        None => Ok(None),
        Some(r) => Ok(Some(load_api_document(&mut *conn, r.id, r.guid).await?)),
    }
}

pub async fn select_document_by_guid(
    conn: &mut SqliteConnection,
    guid: &str,
) -> Result<Option<Document>, Error> {
    sqlx::query_as::<_, Document>("SELECT id, guid FROM documents WHERE guid = ?")
        .bind(guid)
        .fetch_optional(&mut *conn)
        .await
        .map_err(Into::into)
}

async fn load_api_document(
    conn: &mut SqliteConnection,
    document_id: i32,
    guid: String,
) -> Result<ApiDocument, Error> {
    let user_meta = get_document_user_metadata(&mut *conn, document_id).await?;
    let metadata = user_meta.map(DocumentMeta::from_db).unwrap_or_default();
    let file_guids: Vec<String> = sqlx::query_scalar(
        "SELECT f.guid FROM files f
         JOIN contents c ON f.fingerprint = c.fingerprint
         WHERE c.document_id = ?",
    )
    .bind(document_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(ApiDocument {
        guid,
        metadata,
        file_guids,
    })
}

/// Return the cover image for a document: use `selected_cover_fingerprint` when set,
/// otherwise fall back to the first content that has a cover.
pub async fn get_document_selected_cover(
    conn: &mut SqliteConnection,
    document_id: i32,
) -> Result<Option<(Vec<u8>, String)>, Error> {
    // Try the user-selected cover first.
    let selected = sqlx::query_as::<_, (Vec<u8>, String)>(
        "SELECT c.data, c.mime \
         FROM document_metadata dm \
         JOIN covers c ON c.fingerprint = dm.selected_cover_fingerprint \
         WHERE dm.document_id = ?",
    )
    .bind(document_id)
    .fetch_optional(&mut *conn)
    .await?;

    if selected.is_some() {
        return Ok(selected);
    }

    // Fall back to the first content that has a stored cover.
    sqlx::query_as::<_, (Vec<u8>, String)>(
        "SELECT c.data, c.mime \
         FROM contents ct \
         JOIN covers c ON c.fingerprint = ct.fingerprint \
         WHERE ct.document_id = ? \
         LIMIT 1",
    )
    .bind(document_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(Into::into)
}

// ─── Cover queries ────────────────────────────────────────────────────────────

/// Return the set of all fingerprints that have a stored cover image.
pub async fn select_fingerprints_with_covers(
    conn: &mut SqliteConnection,
) -> Result<std::collections::HashSet<String>, Error> {
    let rows: Vec<String> = sqlx::query_scalar("SELECT fingerprint FROM covers")
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows.into_iter().collect())
}

pub async fn upsert_cover(
    conn: &mut SqliteConnection,
    fingerprint: &str,
    data: &[u8],
    mime: &str,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO covers (fingerprint, data, mime) VALUES (?, ?, ?) \
         ON CONFLICT(fingerprint) DO UPDATE SET data = excluded.data, mime = excluded.mime",
    )
    .bind(fingerprint)
    .bind(data)
    .bind(mime)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn get_cover(
    conn: &mut SqliteConnection,
    fingerprint: &str,
) -> Result<Option<(Vec<u8>, String)>, Error> {
    let row = sqlx::query_as::<_, (Vec<u8>, String)>(
        "SELECT data, mime FROM covers WHERE fingerprint = ?",
    )
    .bind(fingerprint)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row)
}

pub async fn cover_exists(conn: &mut SqliteConnection, fingerprint: &str) -> Result<bool, Error> {
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM covers WHERE fingerprint = ?)")
            .bind(fingerprint)
            .fetch_one(&mut *conn)
            .await?;
    Ok(exists)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_pool() -> sqlx::SqlitePool {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrations");
        pool
    }

    #[tokio::test]
    async fn upsert_document_user_metadata_inserts_and_updates() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let doc = upsert_document(&mut conn, "doc-guid-1").await.unwrap();

        let row = upsert_document_user_metadata(
            &mut conn,
            doc.id,
            Some("Book"),
            Some("My Title"),
            None,
            Some(r#"["Alice","Bob"]"#),
            None,
            Some("en"),
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(row.document_id, doc.id);
        assert_eq!(row.title.as_deref(), Some("My Title"));
        assert_eq!(row.document_type.as_deref(), Some("Book"));
        assert_eq!(row.authors.as_deref(), Some(r#"["Alice","Bob"]"#));
        assert_eq!(row.language.as_deref(), Some("en"));

        // Second call must overwrite.
        let updated = upsert_document_user_metadata(
            &mut conn,
            doc.id,
            Some("Article"),
            Some("Updated Title"),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(updated.document_type.as_deref(), Some("Article"));
        assert_eq!(updated.title.as_deref(), Some("Updated Title"));
    }

    #[tokio::test]
    async fn get_document_user_metadata_returns_none_when_absent() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let doc = upsert_document(&mut conn, "doc-guid-2").await.unwrap();
        let result = get_document_user_metadata(&mut conn, doc.id).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn merge_metadata_inserts_when_absent() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let doc = upsert_document(&mut conn, "doc-m1").await.unwrap();

        let meta = ExtractedMetadata {
            title: Some("The Book".into()),
            subtitle: None,
            authors: vec!["Alice".into()],
            description: None,
            language: Some("en".into()),
            publisher: None,
            identifier: None,
            date: None,
            subject: None,
        };
        merge_document_metadata_from_extracted(&mut conn, doc.id, &meta)
            .await
            .unwrap();

        let row = get_document_user_metadata(&mut conn, doc.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.title.as_deref(), Some("The Book"));
        assert_eq!(row.language.as_deref(), Some("en"));
        let authors: Vec<String> = serde_json::from_str(row.authors.as_deref().unwrap()).unwrap();
        assert_eq!(authors, vec!["Alice"]);
    }

    #[tokio::test]
    async fn merge_documents_reassigns_contents_and_deletes_loser() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();

        let winner = upsert_document(&mut conn, "winner-guid").await.unwrap();
        let loser = upsert_document(&mut conn, "loser-guid").await.unwrap();

        // Give the winner a content row.
        upsert_content(&mut conn, "fp-a").await.unwrap();
        sqlx::query("UPDATE contents SET document_id = ? WHERE fingerprint = ?")
            .bind(winner.id)
            .bind("fp-a")
            .execute(&mut *conn)
            .await
            .unwrap();

        // Give the loser a content row.
        upsert_content(&mut conn, "fp-b").await.unwrap();
        sqlx::query("UPDATE contents SET document_id = ? WHERE fingerprint = ?")
            .bind(loser.id)
            .bind("fp-b")
            .execute(&mut *conn)
            .await
            .unwrap();

        // Also give the loser some metadata that should be absorbed by the winner.
        upsert_document_user_metadata(
            &mut conn,
            loser.id,
            Some("Book"),
            Some("Loser Title"),
            None,
            Some(r#"["Loser Author"]"#),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        drop(conn);

        merge_documents(&pool, "winner-guid", &["loser-guid".to_string()])
            .await
            .unwrap();

        let mut conn = pool.acquire().await.unwrap();

        // fp-b must now belong to the winner.
        let doc_id: Option<i32> =
            sqlx::query_scalar("SELECT document_id FROM contents WHERE fingerprint = ?")
                .bind("fp-b")
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert_eq!(doc_id, Some(winner.id));

        // Loser document row must be gone.
        let loser_exists: bool =
            sqlx::query_scalar("SELECT COUNT(*) > 0 FROM documents WHERE guid = ?")
                .bind("loser-guid")
                .fetch_one(&mut *conn)
                .await
                .unwrap();
        assert!(!loser_exists);

        // Winner's metadata must include the loser's title (winner had none).
        let meta = get_document_user_metadata(&mut conn, winner.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(meta.title.as_deref(), Some("Loser Title"));
    }

    #[tokio::test]
    async fn merge_documents_ignores_unknown_guids() {
        let pool = test_pool().await;
        // Should not panic or return an error.
        merge_documents(&pool, "does-not-exist", &["also-missing".to_string()])
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn merge_metadata_keeps_existing_scalars_and_extends_authors() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let doc = upsert_document(&mut conn, "doc-m2").await.unwrap();

        // Insert initial metadata (simulates what scan writes for file A).
        let first = ExtractedMetadata {
            title: Some("The Book".into()),
            subtitle: None,
            authors: vec!["Alice".into()],
            description: None,
            language: Some("en".into()),
            publisher: Some("Pub A".into()),
            identifier: None,
            date: None,
            subject: None,
        };
        merge_document_metadata_from_extracted(&mut conn, doc.id, &first)
            .await
            .unwrap();

        // Merge metadata for a second format of the same book (different author spelling).
        let second = ExtractedMetadata {
            title: Some("The Book (alternate title)".into()),
            subtitle: None,
            authors: vec!["Alice".into(), "Bob".into()],
            description: None,
            language: Some("fr".into()),
            publisher: Some("Pub B".into()),
            identifier: Some("isbn-123".into()),
            date: None,
            subject: None,
        };
        merge_document_metadata_from_extracted(&mut conn, doc.id, &second)
            .await
            .unwrap();

        let row = get_document_user_metadata(&mut conn, doc.id)
            .await
            .unwrap()
            .unwrap();
        // Scalar fields: first value wins.
        assert_eq!(row.title.as_deref(), Some("The Book"));
        assert_eq!(row.language.as_deref(), Some("en"));
        assert_eq!(row.publisher.as_deref(), Some("Pub A"));
        // New scalar that was absent in first merge gets filled.
        assert_eq!(row.identifier.as_deref(), Some("isbn-123"));
        // Authors: extended with new unique entries.
        let authors: Vec<String> = serde_json::from_str(row.authors.as_deref().unwrap()).unwrap();
        assert!(authors.contains(&"Alice".to_string()));
        assert!(authors.contains(&"Bob".to_string()));
        assert_eq!(authors.len(), 2); // "Alice" not duplicated
    }

    // ── File CRUD ─────────────────────────────────────────────────────────────

    async fn make_file(conn: &mut SqliteConnection, path: &str, fingerprint: &str) -> File {
        upsert_content(conn, fingerprint).await.unwrap();
        write_scanned_file(conn, path, "epub", 1000, fingerprint, &[], None)
            .await
            .unwrap();
        select_file_by_path(conn, path).await.unwrap().unwrap()
    }

    #[tokio::test]
    async fn insert_file_round_trips() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let file = make_file(&mut conn, "/books/a.epub", "fp-rt1").await;
        assert_eq!(file.path, "/books/a.epub");
        assert_eq!(file.fingerprint, "fp-rt1");
        assert_eq!(file.type_, "epub");
    }

    #[tokio::test]
    async fn upsert_file_is_idempotent() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        upsert_content(&mut conn, "fp-idem").await.unwrap();
        let make = || NewFile {
            guid: "guid-idem".into(),
            path: "/books/idem.epub".into(),
            type_: "epub".into(),
            size: 42,
            fingerprint: "fp-idem".into(),
            archive_path: None,
            archive_inner_path: None,
        };
        upsert_file(&mut conn, make()).await.unwrap();
        upsert_file(&mut conn, make()).await.unwrap(); // must not error

        let all = select_all_files(&mut conn).await.unwrap();
        assert_eq!(all.len(), 1);
    }

    #[tokio::test]
    async fn select_file_by_id_and_guid_return_same_row() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let file = make_file(&mut conn, "/books/b.epub", "fp-sel").await;
        let by_id = select_file_by_id(&mut conn, file.id)
            .await
            .unwrap()
            .unwrap();
        let by_guid = select_file_by_guid(&mut conn, &file.guid)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(by_id, by_guid);
    }

    #[tokio::test]
    async fn delete_file_record_removes_row() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let file = make_file(&mut conn, "/books/del.epub", "fp-del").await;
        drop(conn);
        delete_file_record(&pool, file.id).await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        assert!(
            select_file_by_id(&mut conn, file.id)
                .await
                .unwrap()
                .is_none()
        );
    }

    // ── write_scanned_file ────────────────────────────────────────────────────

    #[tokio::test]
    async fn write_scanned_file_new_file_returns_true_false() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let (was_new, was_updated) = write_scanned_file(
            &mut conn,
            "/a.epub",
            "epub",
            100,
            "fp-wsf1",
            &["fiction".into()],
            None,
        )
        .await
        .unwrap();
        assert!(was_new);
        assert!(!was_updated);
        let tags = select_content_tags_by_fingerprint(&mut conn, "fp-wsf1")
            .await
            .unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].tag, "fiction");
    }

    #[tokio::test]
    async fn write_scanned_file_unchanged_returns_false_false() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        write_scanned_file(&mut conn, "/b.epub", "epub", 200, "fp-wsf2", &[], None)
            .await
            .unwrap();
        let (was_new, was_updated) =
            write_scanned_file(&mut conn, "/b.epub", "epub", 200, "fp-wsf2", &[], None)
                .await
                .unwrap();
        assert!(!was_new);
        assert!(!was_updated);
    }

    #[tokio::test]
    async fn write_scanned_file_changed_fingerprint_returns_false_true() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        write_scanned_file(&mut conn, "/c.epub", "epub", 300, "fp-wsf3a", &[], None)
            .await
            .unwrap();
        let (was_new, was_updated) =
            write_scanned_file(&mut conn, "/c.epub", "epub", 300, "fp-wsf3b", &[], None)
                .await
                .unwrap();
        assert!(!was_new);
        assert!(was_updated);
        let file = select_file_by_path(&mut conn, "/c.epub")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(file.fingerprint, "fp-wsf3b");
    }

    // ── Content tags ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn upsert_content_tag_deduplicates() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        upsert_content(&mut conn, "fp-tag1").await.unwrap();
        let tag = ContentTag::new("fp-tag1".into(), "sci-fi".into());
        upsert_content_tag(&mut conn, tag.clone()).await.unwrap();
        upsert_content_tag(&mut conn, tag).await.unwrap(); // idempotent
        let tags = select_content_tags_by_fingerprint(&mut conn, "fp-tag1")
            .await
            .unwrap();
        assert_eq!(tags.len(), 1);
    }

    #[tokio::test]
    async fn delete_content_tags_removes_specific_tags() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        upsert_content(&mut conn, "fp-dtag").await.unwrap();
        upsert_many_content_tags(
            &mut conn,
            vec![
                ContentTag::new("fp-dtag".into(), "a".into()),
                ContentTag::new("fp-dtag".into(), "b".into()),
                ContentTag::new("fp-dtag".into(), "c".into()),
            ],
        )
        .await
        .unwrap();
        delete_content_tags(&mut conn, "fp-dtag", vec!["a".into(), "c".into()])
            .await
            .unwrap();
        let remaining = select_content_tags_by_fingerprint(&mut conn, "fp-dtag")
            .await
            .unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].tag, "b");
    }

    #[tokio::test]
    async fn select_all_distinct_tags_returns_sorted_unique() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        upsert_content(&mut conn, "fp-dt1").await.unwrap();
        upsert_content(&mut conn, "fp-dt2").await.unwrap();
        upsert_many_content_tags(
            &mut conn,
            vec![
                ContentTag::new("fp-dt1".into(), "z".into()),
                ContentTag::new("fp-dt1".into(), "a".into()),
                ContentTag::new("fp-dt2".into(), "a".into()), // duplicate tag, different fingerprint
            ],
        )
        .await
        .unwrap();
        let tags = select_all_distinct_tags(&mut conn).await.unwrap();
        assert_eq!(tags, vec!["a", "z"]);
    }

    #[tokio::test]
    async fn select_all_distinct_tags_excluding_filters() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        upsert_content(&mut conn, "fp-dte").await.unwrap();
        upsert_many_content_tags(
            &mut conn,
            vec![
                ContentTag::new("fp-dte".into(), "fiction".into()),
                ContentTag::new("fp-dte".into(), "romance".into()),
                ContentTag::new("fp-dte".into(), "sci-fi".into()),
            ],
        )
        .await
        .unwrap();
        let tags = select_all_distinct_tags_excluding(&mut conn, &["romance".into()])
            .await
            .unwrap();
        assert!(!tags.contains(&"romance".to_string()));
        assert!(tags.contains(&"fiction".to_string()));
        assert!(tags.contains(&"sci-fi".to_string()));
    }

    #[tokio::test]
    async fn select_files_excluding_tags_filters_correctly() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        write_scanned_file(
            &mut conn,
            "/keep.epub",
            "epub",
            1,
            "fp-keep",
            &["allowed".into()],
            None,
        )
        .await
        .unwrap();
        write_scanned_file(
            &mut conn,
            "/skip.epub",
            "epub",
            2,
            "fp-skip",
            &["excluded".into()],
            None,
        )
        .await
        .unwrap();
        let files = select_all_files_excluding_tags(&mut conn, &["excluded".into()])
            .await
            .unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "/keep.epub");
    }

    // ── Reading state ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn get_reading_state_returns_none_when_absent() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let result = get_reading_state(&mut conn, "no-such-fp").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn upsert_reading_state_auto_transitions_unread_to_reading() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        upsert_content(&mut conn, "fp-rs1").await.unwrap();
        let state = ReadingState {
            fingerprint: "fp-rs1".into(),
            status: 0,
            position: String::new(),
            percentage: 0.5,
            last_updated: "2024-01-01T12:00:00Z".into(),
            status_updated_at: "2024-01-01T12:00:00Z".into(),
        };
        let result = upsert_reading_state(&mut conn, state).await.unwrap();
        assert_eq!(result.status, 1); // auto-promoted to Reading
    }

    #[tokio::test]
    async fn upsert_reading_state_auto_transitions_reading_to_read() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        upsert_content(&mut conn, "fp-rs2").await.unwrap();
        // First: create as Reading
        let state = ReadingState {
            fingerprint: "fp-rs2".into(),
            status: 0,
            position: String::new(),
            percentage: 0.5,
            last_updated: "2024-01-01T10:00:00Z".into(),
            status_updated_at: "2024-01-01T10:00:00Z".into(),
        };
        upsert_reading_state(&mut conn, state).await.unwrap();
        // Second: advance to 99% → should become Read
        let state2 = ReadingState {
            fingerprint: "fp-rs2".into(),
            status: 0,
            position: String::new(),
            percentage: 0.99,
            last_updated: "2024-01-01T11:00:00Z".into(),
            status_updated_at: "2024-01-01T11:00:00Z".into(),
        };
        let result = upsert_reading_state(&mut conn, state2).await.unwrap();
        assert_eq!(result.status, 2);
    }

    #[tokio::test]
    async fn upsert_reading_state_stale_timestamp_not_applied() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        upsert_content(&mut conn, "fp-rs3").await.unwrap();
        let fresh = ReadingState {
            fingerprint: "fp-rs3".into(),
            status: 0,
            position: String::new(),
            percentage: 0.5,
            last_updated: "2024-06-01T12:00:00Z".into(),
            status_updated_at: "2024-06-01T12:00:00Z".into(),
        };
        upsert_reading_state(&mut conn, fresh).await.unwrap();
        // Stale update (older timestamp) — must not overwrite
        let stale = ReadingState {
            fingerprint: "fp-rs3".into(),
            status: 0,
            position: "chapter-1".into(),
            percentage: 0.0,
            last_updated: "2024-01-01T00:00:00Z".into(),
            status_updated_at: "2024-01-01T00:00:00Z".into(),
        };
        upsert_reading_state(&mut conn, stale).await.unwrap();
        let result = get_reading_state(&mut conn, "fp-rs3")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result.status, 1); // original Reading status preserved
        assert_eq!(result.percentage, 0.5); // original percentage preserved
    }

    #[tokio::test]
    async fn update_reading_status_only_bypasses_transitions() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        upsert_content(&mut conn, "fp-rs4").await.unwrap();
        // Mark as Read directly (status=2), even with 0% progress
        update_reading_status_only(&mut conn, "fp-rs4", 2)
            .await
            .unwrap();
        let result = get_reading_state(&mut conn, "fp-rs4")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(result.status, 2);
    }

    // ── Remotes ───────────────────────────────────────────────────────────────

    fn new_remote(order: i32, suffix: &str) -> NewRemote {
        NewRemote {
            base_url: format!("https://example.com/{suffix}"),
            order,
            passphrase: "secret".into(),
            user_id: format!("user-{suffix}"),
        }
    }

    #[tokio::test]
    async fn insert_and_select_remote() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let remote = insert_remote(&mut conn, new_remote(0, "a")).await.unwrap();
        assert_eq!(remote.base_url, "https://example.com/a");
        assert_eq!(remote.order, 0);
        let all = select_all_remotes(&mut conn).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, remote.id);
    }

    #[tokio::test]
    async fn update_remote_changes_fields() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let remote = insert_remote(&mut conn, new_remote(0, "b")).await.unwrap();
        update_remote(
            &mut conn,
            remote.id,
            "https://new.example.com",
            "new-user",
            "new-pass",
        )
        .await
        .unwrap();
        let all = select_all_remotes(&mut conn).await.unwrap();
        assert_eq!(all[0].base_url, "https://new.example.com");
        assert_eq!(all[0].user_id, "new-user");
        assert_eq!(all[0].passphrase, "new-pass");
    }

    #[tokio::test]
    async fn delete_remote_reorders_remaining() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let r0 = insert_remote(&mut conn, new_remote(0, "r0")).await.unwrap();
        let r1 = insert_remote(&mut conn, new_remote(1, "r1")).await.unwrap();
        let _r2 = insert_remote(&mut conn, new_remote(2, "r2")).await.unwrap();
        drop(conn);
        delete_remote_by_id(&pool, r1.id).await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let remaining = select_all_remotes(&mut conn).await.unwrap();
        assert_eq!(remaining.len(), 2);
        // Orders must be compact 0,1 with no gaps
        let orders: Vec<i32> = remaining.iter().map(|r| r.order).collect();
        assert_eq!(orders, vec![0, 1]);
        // r0 should still be first
        assert_eq!(remaining[0].id, r0.id);
    }

    #[tokio::test]
    async fn swap_order_of_remotes_swaps_positions() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        let r0 = insert_remote(&mut conn, new_remote(0, "s0")).await.unwrap();
        let r1 = insert_remote(&mut conn, new_remote(1, "s1")).await.unwrap();
        drop(conn);
        swap_order_of_remotes(&pool, &r0, &r1).await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let all = select_all_remotes(&mut conn).await.unwrap();
        // After swap, r1's original url now appears first
        assert_eq!(all[0].base_url, r1.base_url);
        assert_eq!(all[1].base_url, r0.base_url);
    }

    // ── Covers ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn cover_round_trip() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        upsert_content(&mut conn, "fp-cov1").await.unwrap();
        upsert_cover(&mut conn, "fp-cov1", b"image-data", "image/webp")
            .await
            .unwrap();
        let result = get_cover(&mut conn, "fp-cov1").await.unwrap().unwrap();
        assert_eq!(result.0, b"image-data");
        assert_eq!(result.1, "image/webp");
    }

    #[tokio::test]
    async fn cover_upsert_overwrites_existing() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        upsert_content(&mut conn, "fp-cov2").await.unwrap();
        upsert_cover(&mut conn, "fp-cov2", b"old-data", "image/jpeg")
            .await
            .unwrap();
        upsert_cover(&mut conn, "fp-cov2", b"new-data", "image/webp")
            .await
            .unwrap();
        let result = get_cover(&mut conn, "fp-cov2").await.unwrap().unwrap();
        assert_eq!(result.0, b"new-data");
        assert_eq!(result.1, "image/webp");
    }

    #[tokio::test]
    async fn cover_exists_returns_correct_bool() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        upsert_content(&mut conn, "fp-cov3").await.unwrap();
        assert!(!cover_exists(&mut conn, "fp-cov3").await.unwrap());
        upsert_cover(&mut conn, "fp-cov3", b"data", "image/webp")
            .await
            .unwrap();
        assert!(cover_exists(&mut conn, "fp-cov3").await.unwrap());
    }

    #[tokio::test]
    async fn select_fingerprints_with_covers_returns_set() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        upsert_content(&mut conn, "fp-cov4").await.unwrap();
        upsert_content(&mut conn, "fp-cov5").await.unwrap();
        upsert_cover(&mut conn, "fp-cov4", b"d", "image/webp")
            .await
            .unwrap();
        let fps = select_fingerprints_with_covers(&mut conn).await.unwrap();
        assert!(fps.contains("fp-cov4"));
        assert!(!fps.contains("fp-cov5"));
    }

    // ── ensure_document_for_fingerprint ──────────────────────────────────────

    #[tokio::test]
    async fn ensure_document_for_fingerprint_creates_doc_when_absent() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        upsert_content(&mut conn, "fp-edf1").await.unwrap();
        let doc = ensure_document_for_fingerprint(&mut conn, "fp-edf1")
            .await
            .unwrap();
        assert!(!doc.guid.is_empty());
        // A second call must return the same document guid
        let doc2 = ensure_document_for_fingerprint(&mut conn, "fp-edf1")
            .await
            .unwrap();
        assert_eq!(doc.guid, doc2.guid);
    }

    #[tokio::test]
    async fn ensure_document_for_fingerprint_returns_existing_doc() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        upsert_content(&mut conn, "fp-edf2").await.unwrap();
        let existing_doc = upsert_document(&mut conn, "preset-doc-guid").await.unwrap();
        sqlx::query("UPDATE contents SET document_id = ? WHERE fingerprint = ?")
            .bind(existing_doc.id)
            .bind("fp-edf2")
            .execute(&mut *conn)
            .await
            .unwrap();
        let api_doc = ensure_document_for_fingerprint(&mut conn, "fp-edf2")
            .await
            .unwrap();
        assert_eq!(api_doc.guid, "preset-doc-guid");
    }

    // ── auto_link_documents ───────────────────────────────────────────────────

    #[tokio::test]
    async fn auto_link_documents_links_same_stem_different_fingerprints() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        // Two formats of the same book: /books/mybook.epub and /books/mybook.pdf
        write_scanned_file(
            &mut conn,
            "/books/mybook.epub",
            "epub",
            1,
            "fp-link1",
            &[],
            None,
        )
        .await
        .unwrap();
        write_scanned_file(
            &mut conn,
            "/books/mybook.pdf",
            "pdf",
            2,
            "fp-link2",
            &[],
            None,
        )
        .await
        .unwrap();
        drop(conn);
        auto_link_documents(&pool).await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let f1 = select_file_by_path(&mut conn, "/books/mybook.epub")
            .await
            .unwrap()
            .unwrap();
        let f2 = select_file_by_path(&mut conn, "/books/mybook.pdf")
            .await
            .unwrap()
            .unwrap();
        // Both files should now belong to the same document
        assert!(f1.document_guid.is_some());
        assert_eq!(f1.document_guid, f2.document_guid);
    }

    #[tokio::test]
    async fn auto_link_documents_does_not_link_different_stems() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        write_scanned_file(
            &mut conn,
            "/books/alpha.epub",
            "epub",
            1,
            "fp-ns1",
            &[],
            None,
        )
        .await
        .unwrap();
        write_scanned_file(
            &mut conn,
            "/books/beta.epub",
            "epub",
            2,
            "fp-ns2",
            &[],
            None,
        )
        .await
        .unwrap();
        drop(conn);
        auto_link_documents(&pool).await.unwrap();
        let mut conn = pool.acquire().await.unwrap();
        let f1 = select_file_by_path(&mut conn, "/books/alpha.epub")
            .await
            .unwrap()
            .unwrap();
        let f2 = select_file_by_path(&mut conn, "/books/beta.epub")
            .await
            .unwrap()
            .unwrap();
        // Different stems — must remain unlinked (document_guid = None)
        assert!(f1.document_guid.is_none());
        assert!(f2.document_guid.is_none());
    }

    #[tokio::test]
    async fn auto_link_documents_already_linked_is_no_op() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        write_scanned_file(
            &mut conn,
            "/books/same.epub",
            "epub",
            1,
            "fp-al1",
            &[],
            None,
        )
        .await
        .unwrap();
        write_scanned_file(&mut conn, "/books/same.pdf", "pdf", 2, "fp-al2", &[], None)
            .await
            .unwrap();
        drop(conn);
        auto_link_documents(&pool).await.unwrap();
        auto_link_documents(&pool).await.unwrap(); // second run must be a no-op
        let mut conn = pool.acquire().await.unwrap();
        let f1 = select_file_by_path(&mut conn, "/books/same.epub")
            .await
            .unwrap()
            .unwrap();
        let f2 = select_file_by_path(&mut conn, "/books/same.pdf")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(f1.document_guid, f2.document_guid);
        // Count documents — must still be exactly 1
        let doc_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(doc_count, 1);
    }
}
