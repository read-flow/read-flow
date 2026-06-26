//! Steps for `features/_smoke_rest.feature` — the cucumber-rs harness canary.
//! REST-only for now (`@rest`); proves a real backend boots and `RestDriver`
//! can reach it. Mirrors `_smoke.feature`'s role for the PWA harness.

use cucumber::given;
use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

#[given(regex = "^a read-flow server is running$")]
async fn server_is_running(world: &mut BddWorld) {
    assert!(
        world.driver.base_url().starts_with("http://127.0.0.1:"),
        "expected the RestDriver to have booted a server",
    );
}

#[when(regex = "^I check its status$")]
async fn check_its_status(world: &mut BddWorld) {
    world.last_check = Some(world.driver.status_is_healthy().await);
}

#[then(regex = "^the status is healthy$")]
async fn status_is_healthy(world: &mut BddWorld) {
    assert_eq!(world.last_check, Some(true), "expected a healthy status");
}
