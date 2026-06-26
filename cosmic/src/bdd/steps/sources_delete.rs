use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

#[when("I delete the document")]
async fn delete_document(world: &mut BddWorld) {
    let guid = world
        .current_document_guid
        .clone()
        .expect("seed step must run first");
    world.driver.delete_document(&guid).await;
}

#[then("the file no longer appears in the file index")]
async fn file_not_in_index(world: &mut BddWorld) {
    let guid = world
        .current_document_guid
        .as_deref()
        .expect("seed step must run first");
    assert!(
        !world.driver.file_is_listed(guid).await,
        "expected file to be absent from the file index after deletion"
    );
}
