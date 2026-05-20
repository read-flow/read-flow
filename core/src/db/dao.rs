use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::Arc;

use sqlx::SqliteConnection;
use sqlx::SqlitePool;

use crate::db::models::ContentMetadata;
use crate::db::models::ContentTag;
use crate::db::models::Document;
use crate::db::models::DocumentUserMetadata;
use crate::db::models::File;
use crate::db::models::NewFile;
use crate::db::models::NewRemote;
use crate::db::models::ReadingProgress;
use crate::db::models::Remote;

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
const FILE_SELECT: &str = r#"
    SELECT f.id, f.guid, f.path, f.type, f.size, f.fingerprint, c.status,
           d.guid AS document_guid
    FROM files f
    JOIN contents c ON f.fingerprint = c.fingerprint
    LEFT JOIN documents d ON c.document_id = d.id"#;

pub async fn insert_file(conn: &mut SqliteConnection, file: NewFile) -> Result<File, Error> {
    sqlx::query(
        r#"INSERT INTO files (guid, path, "type", size, fingerprint) VALUES (?, ?, ?, ?, ?)"#,
    )
    .bind(&file.guid)
    .bind(&file.path)
    .bind(&file.type_)
    .bind(file.size)
    .bind(&file.fingerprint)
    .execute(&mut *conn)
    .await?;
    let row = select_file_by_path(&mut *conn, &file.path)
        .await?
        .expect("file must exist after insert");
    Ok(row)
}

pub async fn upsert_file(conn: &mut SqliteConnection, file: NewFile) -> Result<(), Error> {
    sqlx::query(
        r#"INSERT OR IGNORE INTO files (guid, path, "type", size, fingerprint) VALUES (?, ?, ?, ?, ?)"#,
    )
    .bind(&file.guid)
    .bind(&file.path)
    .bind(&file.type_)
    .bind(file.size)
    .bind(&file.fingerprint)
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
    sqlx::query_as::<_, File>(&format!("{FILE_SELECT} ORDER BY f.id"))
        .fetch_all(&mut *conn)
        .await
        .map_err(Into::into)
}

pub async fn select_all_files_order_by_type(
    conn: &mut SqliteConnection,
) -> Result<Vec<File>, Error> {
    sqlx::query_as::<_, File>(&format!("{FILE_SELECT} ORDER BY f.type"))
        .fetch_all(&mut *conn)
        .await
        .map_err(Into::into)
}

pub async fn select_all_files_order_by_path(
    conn: &mut SqliteConnection,
) -> Result<Vec<File>, Error> {
    sqlx::query_as::<_, File>(&format!("{FILE_SELECT} ORDER BY f.path"))
        .fetch_all(&mut *conn)
        .await
        .map_err(Into::into)
}

pub async fn select_all_files_order_by_size(
    conn: &mut SqliteConnection,
) -> Result<Vec<File>, Error> {
    sqlx::query_as::<_, File>(&format!("{FILE_SELECT} ORDER BY f.size"))
        .fetch_all(&mut *conn)
        .await
        .map_err(Into::into)
}

pub async fn select_all_files_order_by_fingerprint(
    conn: &mut SqliteConnection,
) -> Result<Vec<File>, Error> {
    sqlx::query_as::<_, File>(&format!("{FILE_SELECT} ORDER BY f.fingerprint"))
        .fetch_all(&mut *conn)
        .await
        .map_err(Into::into)
}

pub async fn select_file_by_id(
    conn: &mut SqliteConnection,
    id: i32,
) -> Result<Option<File>, Error> {
    sqlx::query_as::<_, File>(&format!("{FILE_SELECT} WHERE f.id = ?"))
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(Into::into)
}

pub async fn select_file_by_guid(
    conn: &mut SqliteConnection,
    guid: &str,
) -> Result<Option<File>, Error> {
    sqlx::query_as::<_, File>(&format!("{FILE_SELECT} WHERE f.guid = ?"))
        .bind(guid)
        .fetch_optional(&mut *conn)
        .await
        .map_err(Into::into)
}

pub async fn select_file_by_path(
    conn: &mut SqliteConnection,
    path: &str,
) -> Result<Option<File>, Error> {
    sqlx::query_as::<_, File>(&format!("{FILE_SELECT} WHERE f.path = ?"))
        .bind(path)
        .fetch_optional(&mut *conn)
        .await
        .map_err(Into::into)
}

pub async fn select_all_files_by_path_like(
    conn: &mut SqliteConnection,
    path: &str,
) -> Result<Vec<File>, Error> {
    sqlx::query_as::<_, File>(&format!("{FILE_SELECT} WHERE f.path LIKE ?"))
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
    sqlx::query("INSERT OR IGNORE INTO contents (fingerprint, status) VALUES (?, 0)")
        .bind(fingerprint)
        .execute(&mut *conn)
        .await?;
    Ok(())
}

pub async fn update_content_status(
    conn: &mut SqliteConnection,
    fingerprint: &str,
    status: i32,
) -> Result<(), Error> {
    sqlx::query("UPDATE contents SET status = ? WHERE fingerprint = ?")
        .bind(status)
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
    let mut q = sqlx::query_as::<_, File>(&query);
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
    let mut q = sqlx::query_scalar::<_, String>(&query);
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

/// Post-scan pass: group all known files by `(parent_directory, stem)` and
/// link contents that share a stem — but have distinct fingerprints — to a
/// common `Document`.  Links are strictly additive; nothing is ever unlinked.
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
        let distinct: std::collections::HashSet<&str> =
            files.iter().map(|f| f.fingerprint.as_str()).collect();
        if distinct.len() <= 1 {
            continue;
        }

        // Find an existing document_id in the group, or create a new one.
        let document_id = match files.iter().find_map(|f| f.document_id) {
            Some(id) => id,
            None => {
                let new_guid = uuid::Uuid::new_v4().to_string();
                let doc = upsert_document(&mut conn, &new_guid).await?;
                tracing::debug!(
                    "created document {} for stem group ({} files)",
                    doc.guid,
                    files.len()
                );
                doc.id
            }
        };

        for file in &files {
            set_content_document(&mut conn, &file.fingerprint, document_id).await?;
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

// ─── Reading progress queries ─────────────────────────────────────────────────

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

// ─── Content metadata ────────────────────────────────────────────────────────

/// Insert metadata for a fingerprint. Skips if a row already exists (`INSERT OR IGNORE`).
pub async fn upsert_content_metadata(
    conn: &mut SqliteConnection,
    fingerprint: &str,
    title: Option<&str>,
    authors: Option<&str>,
    language: Option<&str>,
    publisher: Option<&str>,
    identifier: Option<&str>,
    date: Option<&str>,
    subject: Option<&str>,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT OR IGNORE INTO content_metadata \
         (fingerprint, title, authors, language, publisher, identifier, date, subject, extracted_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))",
    )
    .bind(fingerprint)
    .bind(title)
    .bind(authors)
    .bind(language)
    .bind(publisher)
    .bind(identifier)
    .bind(date)
    .bind(subject)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn select_content_metadata(
    conn: &mut SqliteConnection,
    fingerprint: &str,
) -> Result<Option<ContentMetadata>, Error> {
    sqlx::query_as::<_, ContentMetadata>(
        "SELECT fingerprint, title, authors, language, publisher, identifier, date, subject, extracted_at \
         FROM content_metadata WHERE fingerprint = ?",
    )
    .bind(fingerprint)
    .fetch_optional(&mut *conn)
    .await
    .map_err(Into::into)
}

// ─── High-level scan writer ───────────────────────────────────────────────────

/// Write a single scanned file (upsert content + upsert file + add tags).
/// Returns `(was_new, was_updated)`.
pub async fn write_scanned_file(
    conn: &mut SqliteConnection,
    path: &str,
    extension: &str,
    size: i64,
    fingerprint: &str,
    tags: &[String],
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
        "SELECT document_id, document_type, title, subtitle, authors, description, updated_at \
         FROM document_metadata WHERE document_id = ?",
    )
    .bind(document_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(Into::into)
}

pub async fn upsert_document_user_metadata(
    conn: &mut SqliteConnection,
    document_id: i32,
    document_type: Option<&str>,
    title: Option<&str>,
    subtitle: Option<&str>,
    authors: Option<&str>,
    description: Option<&str>,
) -> Result<DocumentUserMetadata, Error> {
    sqlx::query(
        "INSERT INTO document_metadata \
             (document_id, document_type, title, subtitle, authors, description) \
         VALUES (?, ?, ?, ?, ?, ?) \
         ON CONFLICT(document_id) DO UPDATE SET \
             document_type = excluded.document_type, \
             title         = excluded.title, \
             subtitle      = excluded.subtitle, \
             authors       = excluded.authors, \
             description   = excluded.description, \
             updated_at    = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
    )
    .bind(document_id)
    .bind(document_type)
    .bind(title)
    .bind(subtitle)
    .bind(authors)
    .bind(description)
    .execute(&mut *conn)
    .await?;

    let row = get_document_user_metadata(&mut *conn, document_id)
        .await?
        .expect("row must exist after upsert");
    Ok(row)
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
    async fn upsert_content_metadata_inserts_row() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        upsert_content(&mut conn, "fp1").await.unwrap();
        upsert_content_metadata(
            &mut conn,
            "fp1",
            Some("My Title"),
            Some("Alice"),
            Some("en"),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        let row: (String, String, String) = sqlx::query_as(
            "SELECT title, authors, language FROM content_metadata WHERE fingerprint = 'fp1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.0, "My Title");
        assert_eq!(row.1, "Alice");
        assert_eq!(row.2, "en");
    }

    #[tokio::test]
    async fn upsert_content_metadata_is_idempotent() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        upsert_content(&mut conn, "fp2").await.unwrap();
        upsert_content_metadata(
            &mut conn,
            "fp2",
            Some("First"),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
        // Second call must not overwrite.
        upsert_content_metadata(
            &mut conn,
            "fp2",
            Some("Second"),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        let title: (String,) =
            sqlx::query_as("SELECT title FROM content_metadata WHERE fingerprint = 'fp2'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(title.0, "First");
    }

    #[tokio::test]
    async fn upsert_content_metadata_stores_null_fields() {
        let pool = test_pool().await;
        let mut conn = pool.acquire().await.unwrap();
        upsert_content(&mut conn, "fp3").await.unwrap();
        upsert_content_metadata(&mut conn, "fp3", None, None, None, None, None, None, None)
            .await
            .unwrap();

        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM content_metadata WHERE fingerprint = 'fp3'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count.0, 1);
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
        )
        .await
        .unwrap();

        assert_eq!(row.document_id, doc.id);
        assert_eq!(row.title.as_deref(), Some("My Title"));
        assert_eq!(row.document_type.as_deref(), Some("Book"));
        assert_eq!(row.authors.as_deref(), Some(r#"["Alice","Bob"]"#));

        // Second call must overwrite.
        let updated = upsert_document_user_metadata(
            &mut conn,
            doc.id,
            Some("Article"),
            Some("Updated Title"),
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
}
