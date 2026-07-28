// SPDX-License-Identifier: AGPL-3.0-or-later

//! Unit tests for the dao submodules (in-memory SQLite, real migrations).

use assert4rs::Assert;
use sqlx::SqliteConnection;

use super::*;
use crate::db::models::ContentTag;
use crate::db::models::File;
use crate::db::models::NewFile;
use crate::db::models::NewRemote;
use crate::db::models::ReadingState;
use crate::scan::metadata::ExtractedMetadata;

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

    Assert::that(row.document_id).is(doc.id);
    Assert::that(row.title.as_deref()).is_some("My Title");
    Assert::that(row.document_type.as_deref()).is_some("Book");
    Assert::that(row.authors.as_deref()).is_some(r#"["Alice","Bob"]"#);
    Assert::that(row.language.as_deref()).is_some("en");

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
    Assert::that(updated.document_type.as_deref()).is_some("Article");
    Assert::that(updated.title.as_deref()).is_some("Updated Title");
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
    Assert::that(row.title.as_deref()).is_some("The Book");
    Assert::that(row.language.as_deref()).is_some("en");
    let authors: Vec<String> = serde_json::from_str(row.authors.as_deref().unwrap()).unwrap();
    Assert::that(authors).is(vec!["Alice"]);
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
    Assert::that(doc_id).is_some(winner.id);

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
    Assert::that(meta.title.as_deref()).is_some("Loser Title");
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
    Assert::that(row.title.as_deref()).is_some("The Book");
    Assert::that(row.language.as_deref()).is_some("en");
    Assert::that(row.publisher.as_deref()).is_some("Pub A");
    // New scalar that was absent in first merge gets filled.
    Assert::that(row.identifier.as_deref()).is_some("isbn-123");
    // Authors: extended with new unique entries.
    let authors: Vec<String> = serde_json::from_str(row.authors.as_deref().unwrap()).unwrap();
    assert!(authors.contains(&"Alice".to_string()));
    assert!(authors.contains(&"Bob".to_string()));
    Assert::that(authors).has_length(2); // "Alice" not duplicated
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
    Assert::that(file.path).is("/books/a.epub");
    Assert::that(file.fingerprint).is("fp-rt1");
    Assert::that(file.type_).is("epub");
}

#[tokio::test]
async fn insert_file_sets_imported_at() {
    let pool = test_pool().await;
    let mut conn = pool.acquire().await.unwrap();
    let file = make_file(&mut conn, "/books/imported.epub", "fp-imported1").await;
    assert!(
        !file.imported_at.is_empty(),
        "imported_at should be set on insert"
    );
}

#[tokio::test]
async fn write_scanned_file_update_preserves_original_imported_at() {
    let pool = test_pool().await;
    let mut conn = pool.acquire().await.unwrap();
    write_scanned_file(&mut conn, "/c.epub", "epub", 100, "fp-preserve1", &[], None)
        .await
        .unwrap();
    let original = select_file_by_path(&mut conn, "/c.epub")
        .await
        .unwrap()
        .unwrap();

    // Re-scan with a changed fingerprint — triggers the update path.
    upsert_content(&mut conn, "fp-preserve2").await.unwrap();
    write_scanned_file(&mut conn, "/c.epub", "epub", 150, "fp-preserve2", &[], None)
        .await
        .unwrap();
    let updated = select_file_by_path(&mut conn, "/c.epub")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        updated.imported_at, original.imported_at,
        "re-scanning an existing file must not change its original imported_at"
    );
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
    Assert::that(all).has_length(1);
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
    Assert::that(by_id).is(by_guid);
}

#[tokio::test]
async fn delete_file_record_removes_row() {
    let pool = test_pool().await;
    let mut conn = pool.acquire().await.unwrap();
    let file = make_file(&mut conn, "/books/del.epub", "fp-del").await;
    delete_file_record(&mut conn, file.id).await.unwrap();
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
    Assert::that(&tags).has_length(1);
    Assert::that(tags[0].tag.clone()).is("fiction");
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
    Assert::that(file.fingerprint).is("fp-wsf3b");
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
    Assert::that(tags).has_length(1);
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
    Assert::that(&remaining).has_length(1);
    Assert::that(remaining[0].tag.clone()).is("b");
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
    Assert::that(tags).is(vec!["a", "z"]);
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
    Assert::that(&files).has_length(1);
    Assert::that(files[0].path.clone()).is("/keep.epub");
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
    Assert::that(result.status).is(1); // auto-promoted to Reading
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
    Assert::that(result.status).is(2);
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
    Assert::that(result.status).is(1); // original Reading status preserved
    Assert::that(result.percentage).is(0.5); // original percentage preserved
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
    Assert::that(result.status).is(2);
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
    Assert::that(remote.base_url).is("https://example.com/a");
    Assert::that(remote.order).is(0);
    let all = select_all_remotes(&mut conn).await.unwrap();
    Assert::that(&all).has_length(1);
    Assert::that(all[0].id).is(remote.id);
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
    Assert::that(all[0].base_url.clone()).is("https://new.example.com");
    Assert::that(all[0].user_id.clone()).is("new-user");
    Assert::that(all[0].passphrase.clone()).is("new-pass");
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
    Assert::that(&remaining).has_length(2);
    // Orders must be compact 0,1 with no gaps
    let orders: Vec<i32> = remaining.iter().map(|r| r.order).collect();
    Assert::that(orders).is_eq_to(vec![0, 1]);
    // r0 should still be first
    Assert::that(remaining[0].id).is(r0.id);
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
    let urls: Vec<String> = all.iter().map(|r| r.base_url.clone()).collect();
    Assert::that(urls).is_eq_to(vec![r1.base_url, r0.base_url]);
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
    Assert::that(result.0).is(b"image-data");
    Assert::that(result.1).is("image/webp");
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
    Assert::that(result.0).is(b"new-data");
    Assert::that(result.1).is("image/webp");
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
    Assert::that(doc.guid).is(doc2.guid);
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
    Assert::that(api_doc.guid).is("preset-doc-guid");
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
    Assert::that(f1.document_guid).is(f2.document_guid);
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
    Assert::that(f1.document_guid).is(f2.document_guid);
    // Count documents — must still be exactly 1
    let doc_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    Assert::that(doc_count).is(1);
}
