//! Steps for `features/remotes_status.feature` — first scenario shared across
//! drivers (Stage 5.3). REST has no "add remote" concept, so the `When` step
//! maps onto "call /status with these creds" directly; CosmicDriver inserts a
//! `Remote` row and drives `CheckSourceStatus`. Both converge on the same
//! observable: is the source reachable?

use cucumber::given;
use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

#[given(
    regex = "^a read-flow server is running with user \"([^\"]+)\" and passphrase \"([^\"]+)\"$"
)]
async fn server_is_running_with(world: &mut BddWorld, user: String, passphrase: String) {
    assert_eq!(user, crate::bdd::rest_driver::USER);
    assert_eq!(passphrase, crate::bdd::rest_driver::PASSWORD);
    assert!(
        world.driver.base_url().starts_with("http://127.0.0.1:"),
        "expected the driver to have booted a server",
    );
}

#[when(
    regex = "^I add that server as a remote source named \"[^\"]+\" with user \"([^\"]+)\" and passphrase \"([^\"]+)\"$"
)]
async fn add_remote_source(world: &mut BddWorld, user: String, passphrase: String) {
    world.last_check = Some(
        world
            .driver
            .add_remote_and_check_status(&user, &passphrase)
            .await,
    );
}

#[then(regex = "^the remote source \"[^\"]+\" is reported as reachable$")]
async fn reported_as_reachable(world: &mut BddWorld) {
    assert_eq!(
        world.last_check,
        Some(true),
        "expected the remote source to be reachable"
    );
}

#[then(regex = "^the remote source \"[^\"]+\" is reported as unreachable$")]
async fn reported_as_unreachable(world: &mut BddWorld) {
    assert_eq!(
        world.last_check,
        Some(false),
        "expected the remote source to be unreachable"
    );
}
