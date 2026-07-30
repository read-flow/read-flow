//! Steps for `features/admin_local_identity.feature`. COSMIC-only (see the
//! feature's doc comment) — `Driver::bind_local_identity` and friends panic
//! for the `rest` driver.

use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

/// Password for the authorized user this scenario binds local access to —
/// unrelated to the fixed `alice`/`correct-horse` the booted `TestServer`
/// itself uses (see the shared `Given a read-flow server is running...`
/// step); this one only needs to exist in the desktop's own
/// `authorized_users` for the dropdown to offer it.
const LOCAL_IDENTITY_PASSWORD: &str = "local-access-passphrase";

#[when(
    regex = r#"^I bind local access to user "([^"]+)" and record reading progress "([^"]+)" at (\d+)%$"#
)]
async fn bind_and_record(world: &mut BddWorld, user_id: String, position: String, pct: u64) {
    world
        .driver
        .add_user(&user_id, LOCAL_IDENTITY_PASSWORD)
        .await;
    world.driver.bind_local_identity(&user_id).await;
    let fingerprint = world
        .current_document_fingerprint
        .clone()
        .expect("seed step must run first");
    world
        .driver
        .record_local_reading_progress(&fingerprint, &position, pct as f64 / 100.0)
        .await;
}

#[then(regex = r#"^"([^"]+)" is shown that reading progress "([^"]+)" at (\d+)%$"#)]
async fn shown_progress(world: &mut BddWorld, user_id: String, position: String, pct: u64) {
    let fingerprint = world
        .current_document_fingerprint
        .clone()
        .expect("seed step must run first");
    let (got_position, got_percentage) = world
        .driver
        .reading_progress_for_user(&user_id, &fingerprint)
        .await
        .unwrap_or_else(|| panic!("expected reading progress to be recorded for {user_id:?}"));
    assert_eq!(
        got_position, position,
        "expected position {position:?}, got {got_position:?}"
    );
    let expected_percentage = pct as f64 / 100.0;
    assert!(
        (got_percentage - expected_percentage).abs() < 1e-9,
        "expected percentage {expected_percentage}, got {got_percentage}"
    );
}
