//! Steps for `features/admin_authorized_users.feature`. Reuses the
//! `Given a read-flow server is running...` step from `remotes_status`.

use cucumber::given;
use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

// REST/Cosmic manage their own booted backend's users directly — no
// navigation needed (see `admin_scan_directories`'s identical no-op and the
// feature's doc comment). Only the PWA registers the backend as a remote
// source and selects it first; that leg lives entirely in
// `pwa/e2e/steps/admin_authorized_users.steps.ts`.
//
// `And` inherits the preceding step's type — this follows `Given a read-flow
// server is running...`, so it must be registered as `given`, not `when`.
#[given("I am viewing its authorized users")]
async fn viewing_authorized_users(_world: &mut BddWorld) {}

#[when(regex = "^I add a user \"([^\"]+)\" with passphrase \"([^\"]+)\"$")]
async fn add_user(world: &mut BddWorld, user_id: String, password: String) {
    world.driver.add_user(&user_id, &password).await;
}

#[then(regex = "^\"([^\"]+)\" appears in the list of authorized users$")]
async fn user_is_listed(world: &mut BddWorld, user_id: String) {
    assert!(
        world.driver.user_is_listed(&user_id).await,
        "expected {user_id:?} to appear in the list of authorized users"
    );
}
