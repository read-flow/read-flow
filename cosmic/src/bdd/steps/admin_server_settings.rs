//! Steps for `features/admin_server_settings.feature`. Reuses the
//! `Given a read-flow server is running...` step from `remotes_status`.

use cucumber::given;
use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

// REST/Cosmic manage their own booted backend's settings directly — no
// navigation needed to "view" them (see the feature's doc comment). Only the
// PWA must register the backend as a remote source and select it first
// (mirrors `remotes_manage`'s registration step); that leg lives entirely in
// `pwa/e2e/steps/admin_server_settings.steps.ts`.
//
// `And` inherits the preceding step's type — this follows `Given a read-flow
// server is running...`, so it must be registered as `given`, not `when`.
#[given("I am viewing its server settings")]
async fn viewing_settings(_world: &mut BddWorld) {}

#[when("I enable dry-run mode and save")]
async fn enable_dry_run(world: &mut BddWorld) {
    world.driver.enable_dry_run_and_save().await;
}

#[then("dry-run mode is reported as enabled")]
async fn dry_run_enabled(world: &mut BddWorld) {
    assert!(
        world.driver.dry_run_is_enabled().await,
        "expected dry-run mode to be enabled"
    );
}
