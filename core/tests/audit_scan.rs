use std::fs;

use read_flow_core::audit::AuditActor;
use read_flow_core::audit::AuditChannel;
use read_flow_core::audit::AuditContext;
use read_flow_core::db::dao;
use read_flow_core::scan::ScanSettings;
use read_flow_core::scan::Scanner;
use tempfile::TempDir;

#[tokio::test]
async fn scan_records_operation_and_detected_file_event() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("book.epub"), b"book").unwrap();
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let context = AuditContext::new(
        "scan-operation",
        AuditActor::user("peterpaul"),
        AuditChannel::Rest,
    );

    let scanner = Scanner::new(ScanSettings::default());
    let mut progress = scanner
        .scan_with_audit(temp.path().to_path_buf(), pool.clone(), context)
        .await;
    while progress.recv().await.is_some() {}

    let operation = dao::select_audit_operation(&pool, "scan-operation")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        operation.status,
        read_flow_core::audit::OperationStatus::Completed
    );
    let events = dao::select_audit_events(&pool, "scan-operation")
        .await
        .unwrap();
    assert!(
        events
            .iter()
            .any(|event| event.event_type.as_str() == "scan.file_discovered")
    );
}
