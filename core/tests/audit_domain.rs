use read_flow_core::audit::AuditActor;
use read_flow_core::audit::AuditChannel;
use read_flow_core::audit::AuditContext;
use read_flow_core::audit::AuditEventType;
use read_flow_core::audit::AuditOutcome;
use read_flow_core::audit::OperationStatus;
use read_flow_core::audit::OperationType;

#[test]
fn event_type_serializes_to_stable_wire_name() {
    assert_eq!(
        serde_json::to_string(&AuditEventType::ScanFileContentChanged).unwrap(),
        "\"scan.file_content_changed\""
    );
}

#[test]
fn audit_context_serializes_actor_and_channel_without_localized_text() {
    let context = AuditContext::new(
        "operation-1",
        AuditActor::user("peterpaul"),
        AuditChannel::Cosmic,
    );

    let value = serde_json::to_value(context).unwrap();
    assert_eq!(value["actor"]["kind"], "user");
    assert_eq!(value["actor"]["id"], "peterpaul");
    assert_eq!(value["channel"], "cosmic");
    assert!(value.get("message").is_none());
}

#[test]
fn operation_status_only_allows_terminal_transitions_once() {
    assert!(OperationStatus::Started.can_transition_to(OperationStatus::Completed));
    assert!(OperationStatus::Started.can_transition_to(OperationStatus::Failed));
    assert!(OperationStatus::Started.can_transition_to(OperationStatus::Interrupted));
    assert!(!OperationStatus::Completed.can_transition_to(OperationStatus::Started));
    assert!(!OperationStatus::Failed.can_transition_to(OperationStatus::Completed));
}

#[test]
fn scan_completed_with_errors_is_successful_outcome() {
    assert!(AuditOutcome::CompletedWithErrors.is_success());
    assert!(!AuditOutcome::Failed.is_success());
    assert_eq!(OperationType::Scan.as_str(), "scan");
}

#[test]
fn event_parameters_must_be_an_object() {
    let error = AuditEventType::ScanCompleted
        .validate_parameters(&serde_json::json!(["translated sentence"]))
        .unwrap_err();
    assert!(error.contains("scan.completed"));
}
