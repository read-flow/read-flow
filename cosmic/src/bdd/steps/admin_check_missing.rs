use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

#[when("I run the check-missing operation")]
async fn run_check_missing(world: &mut BddWorld) {
    world._check_missing_result = Some(world.driver.check_missing().await);
}

#[then("no files are reported as missing")]
async fn no_files_missing(world: &mut BddWorld) {
    let missing = world
        ._check_missing_result
        .as_ref()
        .expect("check-missing must run first");
    assert!(
        missing.is_empty(),
        "expected no missing files, got: {missing:?}"
    );
}
