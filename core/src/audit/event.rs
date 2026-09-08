// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditActorKind {
    User,
    LocalIdentity,
    LocalSession,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditActor {
    pub kind: AuditActorKind,
    pub id: Option<String>,
}

impl AuditActor {
    pub fn user(id: impl Into<String>) -> Self {
        Self {
            kind: AuditActorKind::User,
            id: Some(id.into()),
        }
    }

    pub fn local_identity(id: impl Into<String>) -> Self {
        Self {
            kind: AuditActorKind::LocalIdentity,
            id: Some(id.into()),
        }
    }

    pub fn local_session() -> Self {
        Self {
            kind: AuditActorKind::LocalSession,
            id: None,
        }
    }

    pub fn system() -> Self {
        Self {
            kind: AuditActorKind::System,
            id: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditChannel {
    Rest,
    Cosmic,
    Cli,
    Internal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditEventType {
    #[serde(rename = "scan.started")]
    ScanStarted,
    #[serde(rename = "scan.completed")]
    ScanCompleted,
    #[serde(rename = "scan.failed")]
    ScanFailed,
    #[serde(rename = "scan.file_discovered")]
    ScanFileDiscovered,
    #[serde(rename = "scan.file_content_changed")]
    ScanFileContentChanged,
    #[serde(rename = "scan.file_missing")]
    ScanFileMissing,
    #[serde(rename = "scan.tags_added")]
    ScanTagsAdded,
    #[serde(rename = "scan.metadata_extraction_failed")]
    ScanMetadataExtractionFailed,
    #[serde(rename = "file.deleted")]
    FileDeleted,
    #[serde(rename = "file.delete_failed")]
    FileDeleteFailed,
    #[serde(rename = "file.record_removed_missing")]
    FileRecordRemovedMissing,
    #[serde(rename = "document.merged")]
    DocumentMerged,
    #[serde(rename = "document.metadata_changed")]
    DocumentMetadataChanged,
    #[serde(rename = "document.tags_added")]
    DocumentTagsAdded,
    #[serde(rename = "document.tags_removed")]
    DocumentTagsRemoved,
    #[serde(rename = "document.cover_changed")]
    DocumentCoverChanged,
    #[serde(rename = "document.status_changed")]
    DocumentStatusChanged,
    #[serde(rename = "maintenance.missing_files_checked")]
    MaintenanceMissingFilesChecked,
    #[serde(rename = "maintenance.missing_files_purged")]
    MaintenanceMissingFilesPurged,
    #[serde(rename = "operation.interrupted")]
    OperationInterrupted,
}

impl AuditEventType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ScanStarted => "scan.started",
            Self::ScanCompleted => "scan.completed",
            Self::ScanFailed => "scan.failed",
            Self::ScanFileDiscovered => "scan.file_discovered",
            Self::ScanFileContentChanged => "scan.file_content_changed",
            Self::ScanFileMissing => "scan.file_missing",
            Self::ScanTagsAdded => "scan.tags_added",
            Self::ScanMetadataExtractionFailed => "scan.metadata_extraction_failed",
            Self::FileDeleted => "file.deleted",
            Self::FileDeleteFailed => "file.delete_failed",
            Self::FileRecordRemovedMissing => "file.record_removed_missing",
            Self::DocumentMerged => "document.merged",
            Self::DocumentMetadataChanged => "document.metadata_changed",
            Self::DocumentTagsAdded => "document.tags_added",
            Self::DocumentTagsRemoved => "document.tags_removed",
            Self::DocumentCoverChanged => "document.cover_changed",
            Self::DocumentStatusChanged => "document.status_changed",
            Self::MaintenanceMissingFilesChecked => "maintenance.missing_files_checked",
            Self::MaintenanceMissingFilesPurged => "maintenance.missing_files_purged",
            Self::OperationInterrupted => "operation.interrupted",
        }
    }

    pub const fn schema_version(self) -> u32 {
        1
    }

    pub fn validate_parameters(self, parameters: &Value) -> Result<(), String> {
        if !parameters.is_object() {
            return Err(format!(
                "{} parameters must be a JSON object",
                self.as_str()
            ));
        }
        Ok(())
    }
}
