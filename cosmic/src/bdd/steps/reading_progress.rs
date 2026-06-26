use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

#[when(regex = r#"^I set the reading progress to (\d+)% at position "([^"]+)"$"#)]
async fn set_reading_progress(world: &mut BddWorld, percentage: u64, position: String) {
    let fingerprint = world
        .current_document_fingerprint
        .clone()
        .expect("seed step must run first");
    world
        .driver
        .set_reading_progress(&fingerprint, &position, percentage as f64 / 100.0)
        .await;
}

#[then(regex = r#"^the reading progress is (\d+)% at "([^"]+)"$"#)]
async fn reading_progress_is(world: &mut BddWorld, expected_pct: u64, expected_pos: String) {
    let fingerprint = world
        .current_document_fingerprint
        .as_deref()
        .expect("seed step must run first");
    let (position, percentage) = world.driver.get_reading_progress(fingerprint).await;
    assert_eq!(
        position, expected_pos,
        "expected position {expected_pos:?}, got {position:?}"
    );
    let expected_float = expected_pct as f64 / 100.0;
    assert!(
        (percentage - expected_float).abs() < 1e-9,
        "expected percentage {expected_float}, got {percentage}"
    );
}
