// SPDX-License-Identifier: AGPL-3.0-or-later

use serde_json::Value;
use sqlx::Row;
use sqlx::SqliteConnection;
use sqlx::SqlitePool;

use super::Error;
use crate::audit::AuditActor;
use crate::audit::AuditActorKind;
use crate::audit::AuditChannel;
use crate::audit::AuditContext;
use crate::audit::AuditEventType;
use crate::audit::AuditOutcome;
use crate::audit::OperationStatus;
use crate::audit::OperationType;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditTargetSnapshot {
    pub target_kind: String,
    pub target_id: String,
    pub title_snapshot: Option<String>,
    pub path_snapshot: Option<String>,
    pub fingerprint_snapshot: Option<String>,
}

impl AuditTargetSnapshot {
    pub fn file(id: impl Into<String>, title: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            target_kind: "file".to_string(),
            target_id: id.into(),
            title_snapshot: Some(title.into()),
            path_snapshot: Some(path.into()),
            fingerprint_snapshot: None,
        }
    }

    pub fn path(path: impl Into<String>) -> Self {
        let path = path.into();
        Self {
            target_kind: "path".to_string(),
            target_id: path.clone(),
            title_snapshot: None,
            path_snapshot: Some(path),
            fingerprint_snapshot: None,
        }
    }

    /// A `document` target. Used for document-level events (merge, metadata,
    /// tags, status, cover) so `/documents/{guid}/activity` can find them even
    /// after the underlying file/rows are gone.
    pub fn document(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            target_kind: "document".to_string(),
            target_id: id.into(),
            title_snapshot: Some(title.into()),
            path_snapshot: None,
            fingerprint_snapshot: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AuditOperationRecord {
    pub id: String,
    pub operation_type: OperationType,
    pub actor: AuditActor,
    pub channel: AuditChannel,
    pub status: OperationStatus,
    pub dry_run: bool,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub error_code: Option<String>,
    pub counts: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AuditEventRecord {
    pub id: i64,
    pub operation_id: String,
    pub event_type: AuditEventType,
    pub schema_version: u32,
    pub sequence: i64,
    pub occurred_at: String,
    pub outcome: AuditOutcome,
    pub parameters: Value,
    pub targets: Vec<AuditTargetSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditCursor {
    pub started_at: String,
    pub operation_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AuditOperationPage {
    pub operations: Vec<AuditOperationRecord>,
    pub next_cursor: Option<AuditCursor>,
}

pub async fn create_audit_operation(
    pool: &SqlitePool,
    context: &AuditContext,
    operation_type: OperationType,
    dry_run: bool,
    started_at: &str,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO audit_operations
         (id, operation_type, actor_kind, actor_id, channel, status, dry_run, started_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&context.operation_id)
    .bind(operation_type.as_str())
    .bind(actor_kind_as_str(context.actor.kind))
    .bind(context.actor.id.as_deref())
    .bind(channel_as_str(context.channel))
    .bind(status_as_str(OperationStatus::Started))
    .bind(dry_run)
    .bind(started_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn append_audit_event(
    connection: &mut SqliteConnection,
    operation_id: &str,
    event_type: AuditEventType,
    outcome: AuditOutcome,
    occurred_at: &str,
    parameters: Value,
    targets: Vec<AuditTargetSnapshot>,
) -> Result<i64, Error> {
    event_type
        .validate_parameters(&parameters)
        .map_err(Error::InvalidAuditEvent)?;
    // Compute the next sequence atomically inside the same statement so two
    // concurrent writers (SQLite serializes writes with a database lock
    // anyway, but the SELECT-then-INSERT split still raced a unique
    // `(operation_id, sequence)` constraint under interleaved transactions).
    let result = sqlx::query(
        "INSERT INTO audit_events
         (operation_id, event_type, schema_version, sequence, occurred_at, outcome, parameters_json)
         SELECT ?, ?, ?, \
         COALESCE((SELECT MAX(sequence) FROM audit_events WHERE operation_id = ?), 0) + 1, ?, ?, ?",
    )
    .bind(operation_id)
    .bind(event_type.as_str())
    .bind(event_type.schema_version() as i64)
    .bind(operation_id)
    .bind(occurred_at)
    .bind(outcome_as_str(outcome))
    .bind(
        serde_json::to_string(&parameters)
            .map_err(|error| Error::InvalidAuditEvent(error.to_string()))?,
    )
    .execute(&mut *connection)
    .await?;
    let event_id = result.last_insert_rowid();
    for target in targets {
        sqlx::query(
            "INSERT INTO audit_targets
             (event_id, target_kind, target_id, title_snapshot, path_snapshot, fingerprint_snapshot)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(event_id)
        .bind(target.target_kind)
        .bind(target.target_id)
        .bind(target.title_snapshot)
        .bind(target.path_snapshot)
        .bind(target.fingerprint_snapshot)
        .execute(&mut *connection)
        .await?;
    }
    Ok(event_id)
}

pub async fn finish_audit_operation(
    pool: &SqlitePool,
    operation_id: &str,
    status: OperationStatus,
    completed_at: &str,
    error_code: Option<&str>,
    counts: Value,
) -> Result<(), Error> {
    let mut connection = pool.acquire().await?;
    finish_audit_operation_on_connection(
        &mut connection,
        operation_id,
        status,
        completed_at,
        error_code,
        counts,
    )
    .await
}

/// Like [`finish_audit_operation`], but against a caller-provided connection so
/// the transition can join the mutation's transaction when the caller wants it
/// to commit/roll back together with the operation's own writes.
pub async fn finish_audit_operation_on_connection(
    connection: &mut SqliteConnection,
    operation_id: &str,
    status: OperationStatus,
    completed_at: &str,
    error_code: Option<&str>,
    counts: Value,
) -> Result<(), Error> {
    let result = sqlx::query(
        "UPDATE audit_operations
         SET status = ?, completed_at = ?, error_code = ?, counts_json = ?
         WHERE id = ? AND status = ?",
    )
    .bind(status_as_str(status))
    .bind(completed_at)
    .bind(error_code)
    .bind(
        serde_json::to_string(&counts)
            .map_err(|error| Error::InvalidAuditEvent(error.to_string()))?,
    )
    .bind(operation_id)
    .bind(status_as_str(OperationStatus::Started))
    .execute(&mut *connection)
    .await?;
    if result.rows_affected() != 1 {
        return Err(Error::InvalidAuditEvent(format!(
            "operation {operation_id} is missing or already terminal"
        )));
    }
    Ok(())
}

/// Mark every operation still in `started` as `interrupted` (startup crash
/// recovery — a previous process died before finishing its operations), and
/// drop an `operation.interrupted` event into each so the history is truthful.
pub async fn interrupt_stale_operations(
    pool: &SqlitePool,
    occurred_at: &str,
) -> Result<u64, Error> {
    let mut connection = pool.acquire().await?;
    let stale: Vec<String> = sqlx::query_scalar("SELECT id FROM audit_operations WHERE status = ?")
        .bind(status_as_str(OperationStatus::Started))
        .fetch_all(&mut *connection)
        .await?;
    for operation_id in &stale {
        if append_audit_event(
            &mut connection,
            operation_id,
            AuditEventType::OperationInterrupted,
            AuditOutcome::Failed,
            occurred_at,
            Value::Object(Default::default()),
            vec![],
        )
        .await
        .is_err()
        {
            // The interruption event is best-effort; still finish the op.
        }
        let _ = finish_audit_operation_on_connection(
            &mut connection,
            operation_id,
            OperationStatus::Interrupted,
            occurred_at,
            Some("interrupted_on_startup"),
            Value::Object(Default::default()),
        )
        .await;
    }
    Ok(stale.len() as u64)
}

pub async fn select_audit_operation(
    pool: &SqlitePool,
    operation_id: &str,
) -> Result<Option<AuditOperationRecord>, Error> {
    let row = sqlx::query("SELECT * FROM audit_operations WHERE id = ?")
        .bind(operation_id)
        .fetch_optional(pool)
        .await?;
    row.map(operation_from_row).transpose()
}

pub async fn select_audit_events(
    pool: &SqlitePool,
    operation_id: &str,
) -> Result<Vec<AuditEventRecord>, Error> {
    let rows = sqlx::query(
        "SELECT id, operation_id, event_type, schema_version, sequence, occurred_at, outcome, parameters_json
         FROM audit_events WHERE operation_id = ? ORDER BY sequence ASC",
    )
    .bind(operation_id)
    .fetch_all(pool)
    .await?;
    let mut events = Vec::with_capacity(rows.len());
    for row in rows {
        let id: i64 = row.try_get("id")?;
        let targets = select_targets(pool, id).await?;
        events.push(AuditEventRecord {
            id,
            operation_id: row.try_get("operation_id")?,
            event_type: event_type_from_str(&row.try_get::<String, _>("event_type")?)?,
            schema_version: row.try_get::<i64, _>("schema_version")? as u32,
            sequence: row.try_get("sequence")?,
            occurred_at: row.try_get("occurred_at")?,
            outcome: outcome_from_str(&row.try_get::<String, _>("outcome")?)?,
            parameters: serde_json::from_str(&row.try_get::<String, _>("parameters_json")?)
                .map_err(|error| Error::InvalidAuditEvent(error.to_string()))?,
            targets,
        });
    }
    Ok(events)
}

pub async fn list_audit_operations(
    pool: &SqlitePool,
    limit: usize,
    cursor: Option<&AuditCursor>,
) -> Result<AuditOperationPage, Error> {
    let limit = limit.clamp(1, 100) as i64;
    let rows = if let Some(cursor) = cursor {
        sqlx::query(
            "SELECT * FROM audit_operations
             WHERE started_at < ? OR (started_at = ? AND id < ?)
             ORDER BY started_at DESC, id DESC LIMIT ?",
        )
        .bind(&cursor.started_at)
        .bind(&cursor.started_at)
        .bind(&cursor.operation_id)
        .bind(limit + 1)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            "SELECT * FROM audit_operations
             ORDER BY started_at DESC, id DESC LIMIT ?",
        )
        .bind(limit + 1)
        .fetch_all(pool)
        .await?
    };
    let has_next = rows.len() > limit as usize;
    let operations = rows
        .into_iter()
        .take(limit as usize)
        .map(operation_from_row)
        .collect::<Result<Vec<_>, _>>()?;
    let next_cursor = has_next.then(|| AuditCursor {
        started_at: operations
            .last()
            .expect("page has a row")
            .started_at
            .clone(),
        operation_id: operations.last().expect("page has a row").id.clone(),
    });
    Ok(AuditOperationPage {
        operations,
        next_cursor,
    })
}

pub async fn find_audit_operations_by_target(
    pool: &SqlitePool,
    target_kind: &str,
    target_id: &str,
) -> Result<Vec<String>, Error> {
    Ok(sqlx::query_scalar(
        "SELECT DISTINCT e.operation_id
         FROM audit_targets t JOIN audit_events e ON e.id = t.event_id
         WHERE t.target_kind = ? AND t.target_id = ?
         ORDER BY e.operation_id",
    )
    .bind(target_kind)
    .bind(target_id)
    .fetch_all(pool)
    .await?)
}

async fn select_targets(
    pool: &SqlitePool,
    event_id: i64,
) -> Result<Vec<AuditTargetSnapshot>, Error> {
    Ok(sqlx::query(
        "SELECT target_kind, target_id, title_snapshot, path_snapshot, fingerprint_snapshot
         FROM audit_targets WHERE event_id = ? ORDER BY target_kind, target_id",
    )
    .bind(event_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| {
        Ok(AuditTargetSnapshot {
            target_kind: row.try_get("target_kind")?,
            target_id: row.try_get("target_id")?,
            title_snapshot: row.try_get("title_snapshot")?,
            path_snapshot: row.try_get("path_snapshot")?,
            fingerprint_snapshot: row.try_get("fingerprint_snapshot")?,
        })
    })
    .collect::<Result<Vec<_>, sqlx::Error>>()?)
}

fn operation_from_row(row: sqlx::sqlite::SqliteRow) -> Result<AuditOperationRecord, Error> {
    Ok(AuditOperationRecord {
        id: row.try_get("id")?,
        operation_type: operation_type_from_str(&row.try_get::<String, _>("operation_type")?)?,
        actor: AuditActor {
            kind: actor_kind_from_str(&row.try_get::<String, _>("actor_kind")?)?,
            id: row.try_get("actor_id")?,
        },
        channel: channel_from_str(&row.try_get::<String, _>("channel")?)?,
        status: status_from_str(&row.try_get::<String, _>("status")?)?,
        dry_run: row.try_get("dry_run")?,
        started_at: row.try_get("started_at")?,
        completed_at: row.try_get("completed_at")?,
        error_code: row.try_get("error_code")?,
        counts: serde_json::from_str(&row.try_get::<String, _>("counts_json")?)
            .map_err(|error| Error::InvalidAuditEvent(error.to_string()))?,
    })
}

fn actor_kind_as_str(kind: AuditActorKind) -> &'static str {
    match kind {
        AuditActorKind::User => "user",
        AuditActorKind::LocalIdentity => "local_identity",
        AuditActorKind::LocalSession => "local_session",
        AuditActorKind::System => "system",
    }
}

fn channel_as_str(channel: AuditChannel) -> &'static str {
    match channel {
        AuditChannel::Rest => "rest",
        AuditChannel::Cosmic => "cosmic",
        AuditChannel::Cli => "cli",
        AuditChannel::Internal => "internal",
    }
}

fn status_as_str(status: OperationStatus) -> &'static str {
    match status {
        OperationStatus::Started => "started",
        OperationStatus::Completed => "completed",
        OperationStatus::Failed => "failed",
        OperationStatus::Interrupted => "interrupted",
    }
}

fn outcome_as_str(outcome: AuditOutcome) -> &'static str {
    match outcome {
        AuditOutcome::Success => "success",
        AuditOutcome::CompletedWithErrors => "completed_with_errors",
        AuditOutcome::Failed => "failed",
        AuditOutcome::Observed => "observed",
        AuditOutcome::Proposed => "proposed",
    }
}

fn actor_kind_from_str(value: &str) -> Result<AuditActorKind, Error> {
    match value {
        "user" => Ok(AuditActorKind::User),
        "local_identity" => Ok(AuditActorKind::LocalIdentity),
        "local_session" => Ok(AuditActorKind::LocalSession),
        "system" => Ok(AuditActorKind::System),
        _ => Err(Error::InvalidAuditEvent(format!(
            "unknown actor kind: {value}"
        ))),
    }
}

fn channel_from_str(value: &str) -> Result<AuditChannel, Error> {
    match value {
        "rest" => Ok(AuditChannel::Rest),
        "cosmic" => Ok(AuditChannel::Cosmic),
        "cli" => Ok(AuditChannel::Cli),
        "internal" => Ok(AuditChannel::Internal),
        _ => Err(Error::InvalidAuditEvent(format!(
            "unknown audit channel: {value}"
        ))),
    }
}

fn status_from_str(value: &str) -> Result<OperationStatus, Error> {
    match value {
        "started" => Ok(OperationStatus::Started),
        "completed" => Ok(OperationStatus::Completed),
        "failed" => Ok(OperationStatus::Failed),
        "interrupted" => Ok(OperationStatus::Interrupted),
        _ => Err(Error::InvalidAuditEvent(format!(
            "unknown operation status: {value}"
        ))),
    }
}

fn outcome_from_str(value: &str) -> Result<AuditOutcome, Error> {
    match value {
        "success" => Ok(AuditOutcome::Success),
        "completed_with_errors" => Ok(AuditOutcome::CompletedWithErrors),
        "failed" => Ok(AuditOutcome::Failed),
        "observed" => Ok(AuditOutcome::Observed),
        "proposed" => Ok(AuditOutcome::Proposed),
        _ => Err(Error::InvalidAuditEvent(format!(
            "unknown audit outcome: {value}"
        ))),
    }
}

fn operation_type_from_str(value: &str) -> Result<OperationType, Error> {
    match value {
        "scan" => Ok(OperationType::Scan),
        "file_delete" => Ok(OperationType::FileDelete),
        "document_merge" => Ok(OperationType::DocumentMerge),
        "document_content_removed" => Ok(OperationType::DocumentContentRemoved),
        "metadata_edit" => Ok(OperationType::MetadataEdit),
        "tag_edit" => Ok(OperationType::TagEdit),
        "status_edit" => Ok(OperationType::StatusEdit),
        "cover_edit" => Ok(OperationType::CoverEdit),
        "missing_file_maintenance" => Ok(OperationType::MissingFileMaintenance),
        _ => Err(Error::InvalidAuditEvent(format!(
            "unknown operation type: {value}"
        ))),
    }
}

fn event_type_from_str(value: &str) -> Result<AuditEventType, Error> {
    match value {
        "scan.started" => Ok(AuditEventType::ScanStarted),
        "scan.completed" => Ok(AuditEventType::ScanCompleted),
        "scan.failed" => Ok(AuditEventType::ScanFailed),
        "scan.file_discovered" => Ok(AuditEventType::ScanFileDiscovered),
        "scan.file_content_changed" => Ok(AuditEventType::ScanFileContentChanged),
        "scan.file_missing" => Ok(AuditEventType::ScanFileMissing),
        "scan.tags_added" => Ok(AuditEventType::ScanTagsAdded),
        "scan.metadata_extraction_failed" => Ok(AuditEventType::ScanMetadataExtractionFailed),
        "file.deleted" => Ok(AuditEventType::FileDeleted),
        "file.delete_failed" => Ok(AuditEventType::FileDeleteFailed),
        "file.record_removed_missing" => Ok(AuditEventType::FileRecordRemovedMissing),
        "document.merged" => Ok(AuditEventType::DocumentMerged),
        "document.content_removed" => Ok(AuditEventType::DocumentContentRemoved),
        "document.metadata_changed" => Ok(AuditEventType::DocumentMetadataChanged),
        "document.tags_added" => Ok(AuditEventType::DocumentTagsAdded),
        "document.tags_removed" => Ok(AuditEventType::DocumentTagsRemoved),
        "document.cover_changed" => Ok(AuditEventType::DocumentCoverChanged),
        "document.status_changed" => Ok(AuditEventType::DocumentStatusChanged),
        "maintenance.missing_files_checked" => Ok(AuditEventType::MaintenanceMissingFilesChecked),
        "maintenance.missing_files_purged" => Ok(AuditEventType::MaintenanceMissingFilesPurged),
        "operation.interrupted" => Ok(AuditEventType::OperationInterrupted),
        _ => Err(Error::InvalidAuditEvent(format!(
            "unknown event type: {value}"
        ))),
    }
}
