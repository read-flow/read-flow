//! Steps for `features/online_library_manage_catalogs.feature`.

use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

#[when(regex = "^I add a catalog \"([^\"]+)\" with search URL \"([^\"]+)\"$")]
async fn add_catalog(world: &mut BddWorld, name: String, search_url: String) {
    world.driver.add_catalog(&name, &search_url).await;
}

#[then(regex = "^\"([^\"]+)\" appears in the list of online library catalogs$")]
async fn catalog_is_listed(world: &mut BddWorld, name: String) {
    assert!(
        world.driver.catalog_is_listed(&name).await,
        "expected {name:?} to appear in the list of online library catalogs"
    );
}

#[when(regex = "^I disable the built-in catalog \"([^\"]+)\"$")]
async fn disable_builtin_catalog(world: &mut BddWorld, id: String) {
    world.driver.disable_builtin_catalog(&id).await;
}

#[then(regex = "^\"([^\"]+)\" no longer appears in the list of enabled online library catalogs$")]
async fn catalog_no_longer_enabled(world: &mut BddWorld, name: String) {
    assert!(
        !world.driver.enabled_catalog_is_listed(&name).await,
        "expected {name:?} to no longer appear in the list of enabled online library catalogs"
    );
}
