use read_flow_core::audit::AuditActor;
use read_flow_core::audit::AuditChannel;
use read_flow_core::audit::AuditContext;
use read_flow_core::audit::AuditEventType;
use read_flow_core::audit::AuditOutcome;
use read_flow_core::audit::OperationStatus;
use read_flow_core::audit::OperationType;
use read_flow_core::db::dao;

async fn pool() -> sqlx::SqlitePool {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn operation_events_and_targets_are_persisted_in_sequence() {
    let pool = pool().await;
    let context = AuditContext::new(
        "operation-1",
        AuditActor::user("peterpaul"),
        AuditChannel::Rest,
    );

    dao::create_audit_operation(
        &pool,
        &context,
        OperationType::Scan,
        false,
        "2026-09-07T10:00:00Z",
    )
    .await
    .unwrap();
    let mut connection = pool.acquire().await.unwrap();
    let event_id = dao::append_audit_event(
        &mut connection,
        "operation-1",
        AuditEventType::ScanFileDiscovered,
        AuditOutcome::Observed,
        "2026-09-07T10:00:01Z",
        serde_json::json!({"file_name": "Dune.epub"}),
        vec![dao::AuditTargetSnapshot::file(
            "file-1",
            "Dune.epub",
            "/books/Dune.epub",
        )],
    )
    .await
    .unwrap();
    drop(connection);

    dao::finish_audit_operation(
        &pool,
        "operation-1",
        OperationStatus::Completed,
        "2026-09-07T10:00:02Z",
        None,
        serde_json::json!({"discovered": 1}),
    )
    .await
    .unwrap();

    let operation = dao::select_audit_operation(&pool, "operation-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(operation.actor.id.as_deref(), Some("peterpaul"));
    assert_eq!(operation.status, OperationStatus::Completed);
    let events = dao::select_audit_events(&pool, "operation-1")
        .await
        .unwrap();
    assert_eq!(events[0].id, event_id);
    assert_eq!(events[0].sequence, 1);
    assert_eq!(events[0].parameters["file_name"], "Dune.epub");
    assert_eq!(
        events[0].targets[0].path_snapshot.as_deref(),
        Some("/books/Dune.epub")
    );
}

#[tokio::test]
async fn operations_are_cursor_paginated_and_targets_remain_findable() {
    let pool = pool().await;
    for (id, started_at) in [
        ("operation-1", "2026-09-07T10:00:00Z"),
        ("operation-2", "2026-09-07T11:00:00Z"),
    ] {
        let context = AuditContext::new(id, AuditActor::local_session(), AuditChannel::Cosmic);
        dao::create_audit_operation(&pool, &context, OperationType::Scan, false, started_at)
            .await
            .unwrap();
    }

    let first = dao::list_audit_operations(&pool, 1, None).await.unwrap();
    assert_eq!(first.operations[0].id, "operation-2");
    let second = dao::list_audit_operations(&pool, 1, first.next_cursor.as_ref())
        .await
        .unwrap();
    assert_eq!(second.operations[0].id, "operation-1");

    let mut connection = pool.acquire().await.unwrap();
    dao::append_audit_event(
        &mut connection,
        "operation-1",
        AuditEventType::FileDeleted,
        AuditOutcome::Success,
        "2026-09-07T10:00:01Z",
        serde_json::json!({}),
        vec![dao::AuditTargetSnapshot::file(
            "file-1",
            "Dune.epub",
            "/books/Dune.epub",
        )],
    )
    .await
    .unwrap();
    drop(connection);
    let matches = dao::find_audit_operations_by_target(&pool, "file", "file-1")
        .await
        .unwrap();
    assert_eq!(matches, vec!["operation-1"]);
}
