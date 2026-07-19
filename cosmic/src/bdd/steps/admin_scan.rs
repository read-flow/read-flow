use cucumber::given;
use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

#[given("a document is available in a configured scan directory")]
async fn prepare_scan_dir(world: &mut BddWorld) {
    world._scan_dir = Some(world.driver.prepare_scan_dir().await);
}

#[when("I trigger a library scan")]
async fn trigger_scan(world: &mut BddWorld) {
    let summary = world.driver.scan_configured().await;
    world._scan_summary = Some(summary);
}

#[then("the scan reports at least 1 document processed")]
async fn scan_processed_at_least_one(world: &mut BddWorld) {
    let summary = world
        ._scan_summary
        .as_ref()
        .expect("scan must have run first");
    assert!(
        summary.processed >= 1,
        "expected at least 1 document processed, got {}",
        summary.processed
    );
}

#[then(regex = r"^the scan report shows (\d+) file added, (\d+) updated, and (\d+) errors$")]
async fn scan_report_shows_counts(world: &mut BddWorld, added: u64, updated: u64, errors: u64) {
    let summary = world
        ._scan_summary
        .as_ref()
        .expect("scan must have run first");
    assert_eq!(
        (summary.added, summary.updated, summary.errors),
        (added, updated, errors),
        "expected (added, updated, errors) = ({added}, {updated}, {errors}), got ({}, {}, {})",
        summary.added,
        summary.updated,
        summary.errors
    );
}
