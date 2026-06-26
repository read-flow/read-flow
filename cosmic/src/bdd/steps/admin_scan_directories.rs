//! Steps for `features/admin_scan_directories.feature`. Reuses the
//! `Given a read-flow server is running...` step from `remotes_status`.

use cucumber::given;
use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

// REST/Cosmic manage their own booted backend's config directly — no
// navigation needed (see `admin_server_settings`'s identical no-op and the
// feature's doc comment). Only the PWA registers the backend as a remote
// source and selects it first; that leg lives entirely in
// `pwa/e2e/steps/admin_scan_directories.steps.ts`.
//
// `And` inherits the preceding step's type — this follows `Given a read-flow
// server is running...`, so it must be registered as `given`, not `when`.
#[given("I am viewing its scan directory configuration")]
async fn viewing_scan_directories(_world: &mut BddWorld) {}

#[when(regex = "^I add \"([^\"]+)\" as a scan directory$")]
async fn add_scan_directory(world: &mut BddWorld, path: String) {
    world.driver.add_scan_directory(&path).await;
}

#[then(regex = "^\"([^\"]+)\" appears in the list of scan directories$")]
async fn directory_is_listed(world: &mut BddWorld, path: String) {
    assert!(
        world.driver.scan_directory_is_listed(&path).await,
        "expected {path:?} to appear in the list of scan directories"
    );
}
