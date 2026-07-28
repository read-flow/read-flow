// SPDX-License-Identifier: AGPL-3.0-or-later

//! Content rows (`contents`) and content tags (`content_tags`).

use sqlx::SqliteConnection;

use super::Error;
use super::files::FILE_SELECT;
use super::files::bind_file_select;
use super::files::select_all_files;
use crate::db::models::ContentTag;
use crate::db::models::File;

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
    user_id: &str,
    excluded: &[String],
) -> Result<Vec<File>, Error> {
    if excluded.is_empty() {
        return select_all_files(conn, user_id).await;
    }
    let placeholders = excluded.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let query = format!(
        "{FILE_SELECT} WHERE NOT EXISTS (
            SELECT 1 FROM content_tags ct
            WHERE ct.fingerprint = f.fingerprint
            AND ct.tag IN ({placeholders})
        )"
    );
    let mut q = bind_file_select(query, user_id);
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
