//! Steps for `features/admin_local_ca.feature`. Reuses the `Given a
//! read-flow server is running...` step from `remotes_status`.

use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

#[when("I generate a local CA-signed certificate")]
async fn generate_local_ca(world: &mut BddWorld) {
    world.last_check = Some(world.driver.generate_local_ca_and_serve_it().await);
}

#[then("the CA root certificate is served at /ca.pem")]
async fn ca_root_is_served(world: &mut BddWorld) {
    assert_eq!(
        world.last_check,
        Some(true),
        "expected /ca.pem to serve the CA root the generation step produced"
    );
}
