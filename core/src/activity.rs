// SPDX-License-Identifier: AGPL-3.0-or-later

//! Activity-history wire types, shared by the REST API (serialization) and by
//! clients (deserialization). Both the embedded server and a remote
//! `FilesClient` produce/consume the same JSON shape, so the COSMIC and PWA
//! surfaces can render one representation of the structured audit trail.
//!
//! Local presentation (the COSMIC page reading the embedded server) also uses
//! these types: `DbClient` builds them from the DAO records in
//! [`crate::db::dao::audit`].

use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

use crate::audit::AuditActor;
use crate::audit::AuditChannel;
use crate::audit::AuditEventType;
use crate::audit::AuditOutcome;
use crate::audit::OperationStatus;
use crate::audit::OperationType;
use crate::db::dao::AuditCursor;
use crate::db::dao::AuditEventRecord;
use crate::db::dao::AuditOperationPage;
use crate::db::dao::AuditOperationRecord;
use crate::db::dao::AuditTargetSnapshot;

/// A page of operations, newest first, with an optional cursor for the next page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityPage {
    pub operations: Vec<ActivityOperation>,
    pub next_cursor: Option<ActivityCursor>,
}

/// Cursor for the next page of [`ActivityPage`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityCursor {
    pub started_at: String,
    pub operation_id: String,
}

/// One recorded operation (a mutation or observation performed by ReadFlow).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityOperation {
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

/// A full operation with its ordered events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityDetail {
    #[serde(flatten)]
    pub operation: ActivityOperation,
    pub events: Vec<ActivityEvent>,
}

/// One observation within an operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityEvent {
    pub id: i64,
    pub operation_id: String,
    pub event_type: AuditEventType,
    pub schema_version: u32,
    pub sequence: i64,
    pub occurred_at: String,
    pub outcome: AuditOutcome,
    pub parameters: Value,
    pub targets: Vec<ActivityTarget>,
}

/// Snapshot of what the event happened to, taken when the event was recorded.
/// The affected document/file may be gone (merge, purge), so locals keep their
/// own copy of the identifying fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityTarget {
    pub target_kind: String,
    pub target_id: String,
    pub title_snapshot: Option<String>,
    pub path_snapshot: Option<String>,
    pub fingerprint_snapshot: Option<String>,
}

impl From<AuditOperationRecord> for ActivityOperation {
    fn from(record: AuditOperationRecord) -> Self {
        Self {
            id: record.id,
            operation_type: record.operation_type,
            actor: record.actor,
            channel: record.channel,
            status: record.status,
            dry_run: record.dry_run,
            started_at: record.started_at,
            completed_at: record.completed_at,
            error_code: record.error_code,
            counts: record.counts,
        }
    }
}

impl From<AuditEventRecord> for ActivityEvent {
    fn from(record: AuditEventRecord) -> Self {
        Self {
            id: record.id,
            operation_id: record.operation_id,
            event_type: record.event_type,
            schema_version: record.schema_version,
            sequence: record.sequence,
            occurred_at: record.occurred_at,
            outcome: record.outcome,
            parameters: record.parameters,
            targets: record
                .targets
                .into_iter()
                .map(ActivityTarget::from)
                .collect(),
        }
    }
}

impl From<AuditTargetSnapshot> for ActivityTarget {
    fn from(snapshot: AuditTargetSnapshot) -> Self {
        Self {
            target_kind: snapshot.target_kind,
            target_id: snapshot.target_id,
            title_snapshot: snapshot.title_snapshot,
            path_snapshot: snapshot.path_snapshot,
            fingerprint_snapshot: snapshot.fingerprint_snapshot,
        }
    }
}

impl From<AuditCursor> for ActivityCursor {
    fn from(cursor: AuditCursor) -> Self {
        Self {
            started_at: cursor.started_at,
            operation_id: cursor.operation_id,
        }
    }
}

impl From<AuditOperationPage> for ActivityPage {
    fn from(page: AuditOperationPage) -> Self {
        Self {
            operations: page
                .operations
                .into_iter()
                .map(ActivityOperation::from)
                .collect(),
            next_cursor: page.next_cursor.map(ActivityCursor::from),
        }
    }
}
