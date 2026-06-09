//! Steps for `features/tags_add.feature`.
use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

#[when(regex = r#"^I add the tag "([^"]+)" to the document$"#)]
async fn add_tag_to_document(world: &mut BddWorld, tag: String) {
    let guid = world
        .current_document_guid
        .as_ref()
        .expect("no current document — seed step must run first")
        .clone();
    world.driver.add_tag_to_document(&guid, &tag).await;
}

#[then(regex = r#"^"([^"]+)" appears in the document's tag list$"#)]
async fn tag_is_in_document_tags(world: &mut BddWorld, tag: String) {
    let guid = world
        .current_document_guid
        .as_ref()
        .expect("no current document — seed step must run first");
    assert!(
        world.driver.document_has_tag(guid, &tag).await,
        "expected {tag:?} to appear in the document's tag list"
    );
}
