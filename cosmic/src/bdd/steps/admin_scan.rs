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
    let processed = world.driver.scan_configured().await;
    world._scan_processed = Some(processed);
}

#[then("the scan reports at least 1 document processed")]
async fn scan_processed_at_least_one(world: &mut BddWorld) {
    let processed = world._scan_processed.expect("scan must have run first");
    assert!(
        processed >= 1,
        "expected at least 1 document processed, got {processed}"
    );
}
