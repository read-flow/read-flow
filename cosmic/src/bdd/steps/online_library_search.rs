//! Steps for `features/online_library_search.feature`.
use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

// `Given a read-flow server is running…` is in common.rs.

#[when(regex = r#"^I search the online library for "([^"]+)"$"#)]
async fn search_online_library(world: &mut BddWorld, query: String) {
    let ok = world.driver.online_library_search_responds(&query).await;
    world.last_check = Some(ok);
}

#[then("the online library search responds successfully")]
async fn search_responds(world: &mut BddWorld) {
    let ok = world.last_check.expect("search must run first");
    assert!(
        ok,
        "online library search endpoint did not respond with 200"
    );
}
