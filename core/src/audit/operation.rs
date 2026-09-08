// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::Deserialize;
use serde::Serialize;

use super::AuditActor;
use super::AuditChannel;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditContext {
    pub operation_id: String,
    pub actor: AuditActor,
    pub channel: AuditChannel,
}

impl AuditContext {
    pub fn new(operation_id: impl Into<String>, actor: AuditActor, channel: AuditChannel) -> Self {
        Self {
            operation_id: operation_id.into(),
            actor,
            channel,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationType {
    Scan,
    FileDelete,
    DocumentMerge,
    MetadataEdit,
    TagEdit,
    StatusEdit,
    CoverEdit,
    MissingFileMaintenance,
}

impl OperationType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Scan => "scan",
            Self::FileDelete => "file_delete",
            Self::DocumentMerge => "document_merge",
            Self::MetadataEdit => "metadata_edit",
            Self::TagEdit => "tag_edit",
            Self::StatusEdit => "status_edit",
            Self::CoverEdit => "cover_edit",
            Self::MissingFileMaintenance => "missing_file_maintenance",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatus {
    Started,
    Completed,
    Failed,
    Interrupted,
}

impl OperationStatus {
    pub const fn is_terminal(self) -> bool {
        !matches!(self, Self::Started)
    }

    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(self, Self::Started) && next.is_terminal()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditOutcome {
    Success,
    CompletedWithErrors,
    Failed,
    Observed,
    Proposed,
}

impl AuditOutcome {
    pub const fn is_success(self) -> bool {
        matches!(
            self,
            Self::Success | Self::CompletedWithErrors | Self::Observed
        )
    }
}
