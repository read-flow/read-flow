//! File removal coordinates filesystem effects with library persistence.

use std::sync::Arc;

use crate::api::File;
use crate::audit::AuditContext;
use crate::audit::AuditEventType;
use crate::audit::AuditOutcome;
use crate::audit::OperationStatus;
use crate::audit::OperationType;
use crate::db::ConnectionPool;
use crate::db::dao;
use crate::db::dao::Error;

/// Remove a library file with an explicit audit context and
/// truthful filesystem outcomes:
/// * a real deletion records `file.deleted` (Success);
/// * a file already missing on disk records `file.record_removed_missing`
///   with `reason: already_missing` (ReadFlow removed the record, not the
///   file — the observation is not attributed as the user's mutation);
/// * an archive member removal records `file.record_removed_missing` with
///   `reason: archive_member`;
/// * a filesystem error records `file.delete_failed` (Failed).
pub(crate) async fn delete_file(
    pool: &ConnectionPool,
    user_id: &str,
    context: &AuditContext,
    file: &File,
) -> Result<(), Error> {
    if let Err(error) = dao::create_audit_operation(
        pool,
        context,
        OperationType::FileDelete,
        false,
        &crate::audit::now_timestamp(),
    )
    .await
    {
        tracing::warn!("could not start delete audit operation: {error}");
    }

    let (event_type, outcome, params, op_status, error_code) = if file.archive_path.is_some() {
        (
            AuditEventType::FileRecordRemovedMissing,
            AuditOutcome::Observed,
            serde_json::json!({ "reason": "archive_member" }),
            OperationStatus::Completed,
            Some("archive_member"),
        )
    } else {
        match tokio::fs::remove_file(&file.path).await {
            Ok(()) => (
                AuditEventType::FileDeleted,
                AuditOutcome::Success,
                serde_json::json!({}),
                OperationStatus::Completed,
                None,
            ),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (
                AuditEventType::FileRecordRemovedMissing,
                AuditOutcome::Observed,
                serde_json::json!({ "reason": "already_missing" }),
                OperationStatus::Completed,
                Some("already_missing"),
            ),
            Err(e) => {
                record_delete_failure(
                    pool,
                    context,
                    file,
                    "filesystem_error",
                    &e.kind().to_string(),
                    false,
                )
                .await;
                tracing::warn!("Failed to delete file from filesystem: {}", e);
                return Err(Error::IO(Arc::new(e)));
            }
        }
    };

    // The filesystem cannot join the SQLite transaction. On retry an
    // absent file still proceeds to record removal.
    let result = async {
        let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
        if let Some(db_file) = dao::select_file_by_guid(&mut tx, user_id, &file.guid).await? {
            dao::delete_file_record(&mut tx, db_file.id).await?;
        }
        dao::append_audit_event(
            &mut tx,
            &context.operation_id,
            event_type,
            outcome,
            &crate::audit::now_timestamp(),
            params,
            vec![delete_target(file)],
        )
        .await?;
        dao::finish_audit_operation_on_connection(
            &mut tx,
            &context.operation_id,
            op_status,
            &crate::audit::now_timestamp(),
            error_code,
            serde_json::json!({}),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
    .await;
    if result.is_err() {
        record_delete_failure(
            pool,
            context,
            file,
            "database_error",
            "record_removal_failed",
            event_type == AuditEventType::FileDeleted,
        )
        .await;
    }
    result
}

async fn record_delete_failure(
    pool: &ConnectionPool,
    context: &AuditContext,
    file: &File,
    code: &str,
    reason: &str,
    removed_from_disk: bool,
) {
    let result: Result<(), Error> = async {
        let mut tx = pool.begin().await?;
        dao::append_audit_event(
            &mut tx,
            &context.operation_id,
            AuditEventType::FileDeleteFailed,
            AuditOutcome::Failed,
            &crate::audit::now_timestamp(),
            serde_json::json!({ "reason": reason, "removed_from_disk": removed_from_disk }),
            vec![delete_target(file)],
        )
        .await?;
        dao::finish_audit_operation_on_connection(
            &mut tx,
            &context.operation_id,
            OperationStatus::Failed,
            &crate::audit::now_timestamp(),
            Some(code),
            serde_json::json!({}),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
    .await;
    if let Err(error) = result {
        tracing::warn!("could not record delete failure: {error}");
    }
}

fn delete_target(file: &File) -> dao::AuditTargetSnapshot {
    dao::AuditTargetSnapshot::file(file.guid.clone(), file.path.clone(), file.path.clone())
}
