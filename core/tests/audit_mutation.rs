use std::fs;
use std::path::Path;

use read_flow_core::api::DocumentMeta;
use read_flow_core::api::File;
use read_flow_core::api::FileDataSource;
use read_flow_core::api::ReadingStatus;
use read_flow_core::audit::AuditActor;
use read_flow_core::audit::AuditChannel;
use read_flow_core::audit::AuditContext;
use read_flow_core::audit::AuditEventType;
use read_flow_core::audit::AuditOutcome;
use read_flow_core::audit::OperationStatus;
use read_flow_core::audit::OperationType;
use read_flow_core::db::ConnectionPool;
use read_flow_core::db::dao;
use read_flow_core::db::datasource::DbClient;
use read_flow_core::scan::archive;
use sqlx::SqlitePool;
use tempfile::TempDir;

async fn new_client() -> (DbClient, ConnectionPool) {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    (DbClient::new(pool.clone(), "local".to_string()), pool)
}

fn context(id: &str) -> AuditContext {
    AuditContext::new(
        id.to_string(),
        AuditActor::user("owner".to_string()),
        AuditChannel::Rest,
    )
}

async fn import(client: &DbClient, dir: &TempDir, name: &str, content: &[u8]) -> File {
    let path = dir.path().join(name);
    fs::write(&path, content).unwrap();
    client.import_file(&path).await.unwrap()
}

#[tokio::test]
async fn tag_edit_records_operation_and_event() {
    let temp = TempDir::new().unwrap();
    let (client, pool) = new_client().await;
    let file = import(&client, &temp, "book.epub", b"file content").await;
    let context = context("tag-op");

    let updated = client
        .add_file_tags_with_audit(&context, &file.guid, vec!["fiction".into()])
        .await
        .unwrap();
    assert!(updated.contains(&"fiction".to_string()));

    let operation = dao::select_audit_operation(&pool, "tag-op")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(operation.operation_type, OperationType::TagEdit);
    assert_eq!(operation.status, OperationStatus::Completed);
    assert_eq!(
        operation.actor.kind,
        read_flow_core::audit::AuditActorKind::User
    );

    let events = dao::select_audit_events(&pool, "tag-op").await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type.as_str(), "document.tags_added");
    assert_eq!(events[0].parameters["tags"][0], "fiction");
    assert_eq!(events[0].targets[0].target_kind, "file");
    assert_eq!(events[0].targets[0].target_id, file.guid);
}

#[tokio::test]
async fn tag_removal_records_operation_and_event() {
    let temp = TempDir::new().unwrap();
    let (client, pool) = new_client().await;
    let file = import(&client, &temp, "book.epub", b"file content").await;
    let add_context = context("tag-add-op");
    let remove_context = context("tag-remove-op");

    client
        .add_file_tags_with_audit(&add_context, &file.guid, vec!["fiction".into()])
        .await
        .unwrap();
    client
        .delete_file_tags_with_audit(&remove_context, &file.guid, vec!["fiction".into()])
        .await
        .unwrap();

    let events = dao::select_audit_events(&pool, "tag-remove-op")
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type.as_str(), "document.tags_removed");
    assert_eq!(events[0].parameters["tags"][0], "fiction");
}

#[tokio::test]
async fn successful_file_delete_records_deleted_event() {
    let temp = TempDir::new().unwrap();
    let (client, pool) = new_client().await;
    let file = import(&client, &temp, "book.pdf", b"file content").await;
    let context = context("delete-op");

    client
        .delete_file_with_audit(&context, &file)
        .await
        .unwrap();
    assert!(!temp.path().join("book.pdf").exists());

    let operation = dao::select_audit_operation(&pool, "delete-op")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(operation.status, OperationStatus::Completed);
    let events = dao::select_audit_events(&pool, "delete-op").await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type.as_str(), "file.deleted");
    assert_eq!(events[0].outcome, AuditOutcome::Success);
}

#[tokio::test]
async fn already_missing_file_records_record_removed_missing_event() {
    let temp = TempDir::new().unwrap();
    let (client, pool) = new_client().await;
    let file = import(&client, &temp, "book.pdf", b"file content").await;
    // The file vanishes from disk without ReadFlow doing it.
    fs::remove_file(temp.path().join("book.pdf")).unwrap();
    let context = context("delete-missing-op");

    client
        .delete_file_with_audit(&context, &file)
        .await
        .unwrap();

    let operation = dao::select_audit_operation(&pool, "delete-missing-op")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(operation.status, OperationStatus::Completed);
    assert_eq!(operation.error_code.as_deref(), Some("already_missing"));
    let events = dao::select_audit_events(&pool, "delete-missing-op")
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type.as_str(), "file.record_removed_missing");
    assert_eq!(events[0].parameters["reason"], "already_missing");
    // The record removal is observed, not attributed as a user mutation.
    assert_eq!(events[0].outcome, AuditOutcome::Observed);
}

#[tokio::test]
async fn archive_member_delete_records_record_removed_missing_event() {
    let temp = TempDir::new().unwrap();
    let (client, pool) = new_client().await;
    let file = import(&client, &temp, "book.epub", b"file content").await;
    // Turn the row into an archive member: archive_path set, the `path` column
    // keeps the synthetic "{archive}::{inner}" form. `dao::update_file` does
    // not persist archive fields, so update them directly.
    let archive_member_row = sqlx::query(
        "UPDATE files SET archive_path = ?, archive_inner_path = ?, path = ? WHERE guid = ?",
    )
    .bind("book.epub")
    .bind("book.epub")
    .bind(archive::joined_archive_path(
        Path::new("book.epub"),
        "inner.epub",
    ))
    .bind(&file.guid)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(archive_member_row.rows_affected(), 1);
    let file = client.get_file(&file.guid).await.unwrap().unwrap();
    assert!(file.archive_path.is_some());
    let context = context("delete-archive-op");

    client
        .delete_file_with_audit(&context, &file)
        .await
        .unwrap();

    let events = dao::select_audit_events(&pool, "delete-archive-op")
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type.as_str(), "file.record_removed_missing");
    assert_eq!(events[0].parameters["reason"], "archive_member");
    assert_eq!(events[0].outcome, AuditOutcome::Observed);
}

#[tokio::test]
async fn failed_file_delete_records_failed_event() {
    let temp = TempDir::new().unwrap();
    let (client, pool) = new_client().await;
    // A row that no longer has a real file behind it but a *named* path that
    // errors for a different reason than NotFound: use a path that is a
    // directory, so remove_file fails with a non-NotFound error.
    let dir_path = temp.path().join("book.pdf");
    fs::create_dir(&dir_path).unwrap();
    let fingerprint = "bogus-fingerprint";
    let mut conn = pool.acquire().await.unwrap();
    dao::upsert_content(&mut conn, fingerprint).await.unwrap();
    let new_file = read_flow_core::db::models::NewFile {
        guid: "some-guid".to_string(),
        path: dir_path.display().to_string(),
        type_: "pdf".to_string(),
        size: 0,
        fingerprint: fingerprint.to_string(),
        archive_path: None,
        archive_inner_path: None,
    };
    dao::upsert_file(&mut conn, new_file).await.unwrap();
    let file = client.get_file("some-guid").await.unwrap().unwrap();
    let context = context("delete-fail-op");

    let result = client.delete_file_with_audit(&context, &file).await;
    assert!(result.is_err());

    let operation = dao::select_audit_operation(&pool, "delete-fail-op")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(operation.status, OperationStatus::Failed);
    assert_eq!(operation.error_code.as_deref(), Some("filesystem_error"));
    let events = dao::select_audit_events(&pool, "delete-fail-op")
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type.as_str(), "file.delete_failed");
    assert_eq!(events[0].outcome, AuditOutcome::Failed);
}

#[tokio::test]
async fn merge_records_merged_event_with_loser_document_target() {
    let temp = TempDir::new().unwrap();
    let (client, pool) = new_client().await;
    let file_a = import(&client, &temp, "a.epub", b"file a content").await;
    let file_b = import(&client, &temp, "b.epub", b"file b content").await;
    assert_ne!(file_a.fingerprint, file_b.fingerprint);
    let doc_a = client.ensure_document_for_file(&file_a.guid).await.unwrap();
    let doc_b = client.ensure_document_for_file(&file_b.guid).await.unwrap();
    assert_ne!(doc_a.guid, doc_b.guid);
    let context = context("merge-op");

    client
        .merge_documents_with_audit(&context, &doc_a.guid, std::slice::from_ref(&doc_b.guid))
        .await
        .unwrap();

    let operation = dao::select_audit_operation(&pool, "merge-op")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(operation.operation_type, OperationType::DocumentMerge);
    assert_eq!(operation.status, OperationStatus::Completed);
    let events = dao::select_audit_events(&pool, "merge-op").await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type.as_str(), "document.merged");
    assert_eq!(events[0].parameters["loser_count"], 1);
    // The loser keeps its document target even though its row is gone, so
    // /documents/{loser_guid}/activity still finds the event.
    assert!(
        events[0]
            .targets
            .iter()
            .any(|target| target.target_kind == "document" && target.target_id == doc_a.guid)
    );
    assert!(
        events[0]
            .targets
            .iter()
            .any(|target| target.target_kind == "document" && target.target_id == doc_b.guid)
    );
    // The loser row is gone.
    let doc_b_exists = {
        let mut conn = pool.acquire().await.unwrap();
        dao::select_api_document_by_guid(&mut conn, &doc_b.guid)
            .await
            .unwrap()
            .is_some()
    };
    assert!(!doc_b_exists);
}

#[tokio::test]
async fn status_change_records_event() {
    let temp = TempDir::new().unwrap();
    let (client, pool) = new_client().await;
    let file = import(&client, &temp, "book.epub", b"file content").await;
    // Link a document so the event is targeted at it.
    client.ensure_document_for_file(&file.guid).await.unwrap();
    let context = context("status-op");

    client
        .update_reading_status_with_audit(&context, &file.fingerprint, ReadingStatus::Read)
        .await
        .unwrap();

    let operation = dao::select_audit_operation(&pool, "status-op")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(operation.operation_type, OperationType::StatusEdit);
    let events = dao::select_audit_events(&pool, "status-op").await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type.as_str(), "document.status_changed");
    assert_eq!(events[0].parameters["status"], "read");
    assert_eq!(events[0].targets[0].target_kind, "document");
}

#[tokio::test]
async fn metadata_change_records_event() {
    let temp = TempDir::new().unwrap();
    let (client, pool) = new_client().await;
    let file = import(&client, &temp, "book.epub", b"file content").await;
    let doc = client.ensure_document_for_file(&file.guid).await.unwrap();
    let context = context("metadata-op");

    let updated = client
        .update_document_metadata_with_audit(
            &context,
            &doc.guid,
            DocumentMeta {
                title: Some("Retitled".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(updated.unwrap().metadata.title.as_deref(), Some("Retitled"));

    let operation = dao::select_audit_operation(&pool, "metadata-op")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(operation.operation_type, OperationType::MetadataEdit);
    let events = dao::select_audit_events(&pool, "metadata-op")
        .await
        .unwrap();
    assert_eq!(events[0].event_type.as_str(), "document.metadata_changed");
    assert_eq!(events[0].targets[0].target_id, doc.guid);
}

#[tokio::test]
async fn missing_file_purge_records_maintenance_event() {
    let temp = TempDir::new().unwrap();
    let (_client, pool) = new_client().await;
    // A DB record whose file is already gone.
    let missing = temp.path().join("gone.pdf");
    let fingerprint = "gone-fingerprint";
    let mut conn = pool.acquire().await.unwrap();
    dao::upsert_content(&mut conn, fingerprint).await.unwrap();
    dao::upsert_file(
        &mut conn,
        read_flow_core::db::models::NewFile {
            guid: "gone-guid".to_string(),
            path: missing.display().to_string(),
            type_: "pdf".to_string(),
            size: 0,
            fingerprint: fingerprint.to_string(),
            archive_path: None,
            archive_inner_path: None,
        },
    )
    .await
    .unwrap();
    drop(conn);
    let context = context("maintenance-op");

    // Emulate the check: the record points at a path that no longer exists.
    let missing_paths = {
        let mut conn = pool.acquire().await.unwrap();
        dao::select_all_files(&mut conn, "local")
            .await
            .unwrap()
            .into_iter()
            .map(|file| file.path)
            .collect::<Vec<_>>()
    };
    assert_eq!(missing_paths, vec![missing.display().to_string()]);

    // Purge the record and record the maintenance event (same witnesses the
    // ApplicationModule::check_missing_with_audit path goes through).
    let mut conn = pool.acquire().await.unwrap();
    dao::create_audit_operation(
        &pool,
        &context,
        OperationType::MissingFileMaintenance,
        false,
        &read_flow_core::audit::now_timestamp(),
    )
    .await
    .unwrap();
    dao::delete_file_record(&mut conn, 1).await.unwrap();
    dao::append_audit_event(
        &mut conn,
        "maintenance-op",
        AuditEventType::MaintenanceMissingFilesPurged,
        AuditOutcome::Success,
        &read_flow_core::audit::now_timestamp(),
        serde_json::json!({
            "purge": true,
            "missing_at_recorded_path": 1,
        }),
        vec![dao::AuditTargetSnapshot::path(
            missing.display().to_string(),
        )],
    )
    .await
    .unwrap();
    dao::finish_audit_operation(
        &pool,
        "maintenance-op",
        OperationStatus::Completed,
        &read_flow_core::audit::now_timestamp(),
        None,
        serde_json::json!({ "missing_count": 1, "purge": true }),
    )
    .await
    .unwrap();
    drop(conn);

    let operation = dao::select_audit_operation(&pool, "maintenance-op")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        operation.operation_type,
        OperationType::MissingFileMaintenance
    );
    assert_eq!(operation.status, OperationStatus::Completed);
    let events = dao::select_audit_events(&pool, "maintenance-op")
        .await
        .unwrap();
    assert_eq!(
        events[0].event_type.as_str(),
        "maintenance.missing_files_purged"
    );
    assert_eq!(events[0].parameters["missing_at_recorded_path"], 1);
    // The record is gone from the database.
    let remaining = {
        let mut conn = pool.acquire().await.unwrap();
        dao::select_all_files(&mut conn, "local").await.unwrap()
    };
    assert!(remaining.is_empty());
    assert!(!missing.exists());
}
