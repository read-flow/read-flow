//! Steps for `features/online_library_download_import.feature`.
use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

// `Given a read-flow server is running…` is in common.rs.

#[when("I import a book from the online library")]
async fn import_book(world: &mut BddWorld) {
    let ok = world.driver.online_library_import_responds().await;
    world.last_check = Some(ok);
}

#[then("the book was imported successfully")]
async fn book_imported(world: &mut BddWorld) {
    let ok = world.last_check.expect("import must run first");
    assert!(
        ok,
        "online library import endpoint did not respond with 200"
    );
}
