use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

#[when("I enable private mode")]
async fn enable_private_mode(world: &mut BddWorld) {
    world.driver.enable_private_mode().await;
}

#[then("private mode is reported as enabled")]
async fn private_mode_is_enabled(world: &mut BddWorld) {
    assert!(
        world.driver.private_mode_is_enabled().await,
        "expected private mode to be enabled"
    );
}
