//! Steps for `features/remotes_manage.feature` (`@pwa`/`@cosmic` only — no
//! REST surface, see the feature's doc comment). Reuses the
//! `Given a read-flow server is running...` step from `remotes_status`.

use cucumber::given;
use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

// `And` inherits the preceding step's type — this follows `Given a read-flow
// server is running...`, so it must be registered as `given`, not `when`.
#[given(
    regex = "^that server is registered as a remote source with user \"([^\"]+)\" and passphrase \"([^\"]+)\"$"
)]
async fn register_remote(world: &mut BddWorld, user: String, passphrase: String) {
    world.driver.register_remote(&user, &passphrase).await;
}

#[when("I remove that remote source")]
async fn remove_remote(world: &mut BddWorld) {
    world.driver.remove_registered_remote().await;
}

#[then("the list of remote sources is empty")]
async fn list_is_empty(world: &mut BddWorld) {
    assert_eq!(
        world.driver.remote_count().await,
        0,
        "expected no remote sources to remain"
    );
}
