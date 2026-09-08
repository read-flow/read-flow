//! Steps for `features/activity_history.feature`.
//!
//! The activity history renders the structured audit trail ReadFlow keeps for
//! scans and library mutations. Most `Given`/`When` steps here are shared with
//! other features; these steps assert on the recorded history itself.

use cucumber::given;
use cucumber::then;

use crate::bdd::world::BddWorld;

#[given("scan dry-run is enabled")]
async fn enable_scan_dry_run(world: &mut BddWorld) {
    world.driver.enable_dry_run_and_save().await;
}

#[then("the most recent activity is a scan operation")]
async fn most_recent_activity_is_a_scan(world: &mut BddWorld) {
    let kinds = world
        .eventually(
            || world.driver.activity_operation_types(),
            |kinds| kinds.first().is_some_and(|kind| kind == "scan"),
        )
        .await
        .expect("activity history was never observed");
    assert!(
        kinds.first().is_some_and(|kind| kind == "scan"),
        "expected the newest activity operation to be a scan, got {kinds:?}"
    );
}

#[then("the scan activity includes a file-discovered event")]
async fn scan_activity_includes_file_discovered(world: &mut BddWorld) {
    let types = world
        .eventually(
            || world.driver.latest_activity_event_types(),
            |types| types.iter().any(|kind| kind == "scan.file_discovered"),
        )
        .await
        .expect("scan activity events were never observed");
    assert!(
        types.iter().any(|kind| kind == "scan.file_discovered"),
        "expected a scan.file_discovered event, got {types:?}"
    );
}

#[then("that scan is marked as a dry run")]
async fn scan_is_a_dry_run(world: &mut BddWorld) {
    let dry_run = world
        .eventually(
            || world.driver.latest_activity_is_dry_run(),
            |is_dry_run| *is_dry_run,
        )
        .await
        .expect("dry-run flag was never observed");
    assert!(
        dry_run,
        "expected the newest scan operation to be a dry run"
    );
}

#[then("the scan activity events are proposals")]
async fn scan_events_are_proposals(world: &mut BddWorld) {
    let outcomes = world
        .eventually(
            || world.driver.latest_activity_event_outcomes(),
            |outcomes| outcomes.iter().any(|outcome| outcome == "proposed"),
        )
        .await
        .expect("scan activity outcomes were never observed");
    assert!(
        outcomes.iter().any(|outcome| outcome == "proposed"),
        "expected at least one proposed (dry-run) event, got {outcomes:?}"
    );
}

#[then("the merged-away document still has activity")]
async fn merged_away_document_has_activity(world: &mut BddWorld) {
    let guid = world
        .second_document_api_guid
        .as_deref()
        .expect("merged-away document must be seeded first");
    let events = world
        .eventually(
            || world.driver.document_activity_event_types(guid),
            |events| !events.is_empty(),
        )
        .await
        .expect("document activity was never observed");
    assert!(!events.is_empty(), "merged-away document has no activity");
}

#[then("that activity includes a document-merge event")]
async fn activity_includes_document_merge(world: &mut BddWorld) {
    let guid = world
        .second_document_api_guid
        .as_deref()
        .expect("merged-away document must be seeded first");
    let events = world
        .eventually(
            || world.driver.document_activity_event_types(guid),
            |events| events.iter().any(|event| event == "document.merged"),
        )
        .await
        .expect("document activity was never observed");
    assert!(
        events.iter().any(|event| event == "document.merged"),
        "expected a document.merged event, got {events:?}"
    );
}

#[then("the most recent activity is a file-delete operation")]
async fn most_recent_activity_is_a_file_delete(world: &mut BddWorld) {
    let kinds = world
        .eventually(
            || world.driver.activity_operation_types(),
            |kinds| kinds.first().is_some_and(|kind| kind == "file_delete"),
        )
        .await
        .expect("activity history was never observed");
    assert!(
        kinds.first().is_some_and(|kind| kind == "file_delete"),
        "expected the newest activity operation to be a file delete, got {kinds:?}"
    );
}

#[then("the delete activity records a file-deleted event")]
async fn delete_activity_records_file_deleted(world: &mut BddWorld) {
    let types = world
        .eventually(
            || world.driver.latest_activity_event_types(),
            |types| types.iter().any(|kind| kind == "file.deleted"),
        )
        .await
        .expect("delete activity events were never observed");
    assert!(
        types.iter().any(|kind| kind == "file.deleted"),
        "expected a file.deleted event, got {types:?}"
    );
}
