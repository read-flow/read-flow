//! Reading-state commands shared by the desktop and HTTP adapters.

use sqlx::SqliteConnection;

use crate::api::ReadingState;
use crate::api::ReadingStatus;
use crate::audit::AuditContext;
use crate::audit::AuditEventType;
use crate::audit::AuditOutcome;
use crate::audit::OperationStatus;
use crate::audit::OperationType;
use crate::audit::now_timestamp;
use crate::db::ConnectionPool;
use crate::db::dao;
use crate::db::dao::Error;

pub(crate) struct ReadingScope<'a> {
    pub user_id: &'a str,
    pub hidden_tags: &'a [String],
}

async fn visible(
    conn: &mut SqliteConnection,
    scope: &ReadingScope<'_>,
    fingerprint: &str,
) -> Result<bool, Error> {
    if scope.hidden_tags.is_empty() {
        return Ok(true);
    }
    let tags = dao::select_content_tags_by_fingerprint(conn, fingerprint).await?;
    Ok(!tags.iter().any(|tag| scope.hidden_tags.contains(&tag.tag)))
}

/// Save progress for the caller. A hidden fingerprint yields no result.
pub(crate) async fn save_position(
    pool: &ConnectionPool,
    scope: ReadingScope<'_>,
    state: ReadingState,
) -> Result<Option<ReadingState>, Error> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    if !visible(&mut tx, &scope, &state.fingerprint).await? {
        return Ok(None);
    }
    let state = dao::upsert_reading_state(&mut tx, scope.user_id, state).await?;
    tx.commit().await?;
    Ok(Some(state))
}

/// Change status and record its audit event in the same transaction.
/// A hidden fingerprint yields false without writing anything.
pub(crate) async fn set_status(
    pool: &ConnectionPool,
    scope: ReadingScope<'_>,
    context: &AuditContext,
    fingerprint: &str,
    status: ReadingStatus,
) -> Result<bool, Error> {
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    if !visible(&mut tx, &scope, fingerprint).await? {
        return Ok(false);
    }
    dao::create_audit_operation(
        &mut *tx,
        context,
        OperationType::StatusEdit,
        false,
        &now_timestamp(),
    )
    .await?;
    dao::update_reading_status_only(&mut tx, scope.user_id, fingerprint, status.into()).await?;
    let target = dao::document_target(&mut tx, fingerprint)
        .await
        .unwrap_or_else(|| dao::AuditTargetSnapshot::path(fingerprint.to_string()));
    let params = serde_json::json!({ "status": status_str(status) });
    dao::append_audit_event(
        &mut tx,
        &context.operation_id,
        AuditEventType::DocumentStatusChanged,
        AuditOutcome::Success,
        &now_timestamp(),
        params.clone(),
        vec![target],
    )
    .await?;
    dao::finish_audit_operation_on_connection(
        &mut tx,
        &context.operation_id,
        OperationStatus::Completed,
        &now_timestamp(),
        None,
        params,
    )
    .await?;
    tx.commit().await?;
    Ok(true)
}

/// Stable lowercase wire value for a reading status (used in audit parameters).
fn status_str(status: ReadingStatus) -> &'static str {
    match status {
        ReadingStatus::Unread => "unread",
        ReadingStatus::Reading => "reading",
        ReadingStatus::Read => "read",
    }
}
