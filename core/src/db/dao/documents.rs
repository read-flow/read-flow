// SPDX-License-Identifier: AGPL-3.0-or-later

//! Documents, document metadata, linking, merging, and `ApiDocument` loads.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use sqlx::SqliteConnection;
use sqlx::SqlitePool;

use super::Error;
use super::files::select_file_by_guid;
use crate::api::ApiDocument;
use crate::api::DocumentMeta;
use crate::db::models::Document;
use crate::db::models::DocumentUserMetadata;
use crate::scan::metadata::ExtractedMetadata;

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
        .ok_or_else(|| Error::Sqlx(Arc::new(sqlx::Error::RowNotFound)))?;
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
    user_id: &str,
    file_guid: &str,
) -> Result<ApiDocument, Error> {
    let file = select_file_by_guid(&mut *conn, user_id, file_guid)
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
