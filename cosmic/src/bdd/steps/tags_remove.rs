//! Steps for `features/tags_remove.feature`.
use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

#[when(regex = r#"^I remove the tag "([^"]+)" from the document$"#)]
async fn remove_tag_from_document(world: &mut BddWorld, tag: String) {
    let guid = world
        .current_document_guid
        .as_ref()
        .expect("no current document — seed step must run first")
        .clone();
    world.driver.remove_tag_from_document(&guid, &tag).await;
}

#[then(regex = r#"^"([^"]+)" no longer appears in the document's tag list$"#)]
async fn tag_not_in_document_tags(world: &mut BddWorld, tag: String) {
    let guid = world
        .current_document_guid
        .as_ref()
        .expect("no current document — seed step must run first");
    assert!(
        !world.driver.document_has_tag(guid, &tag).await,
        "expected {tag:?} to NOT appear in the document's tag list"
    );
}
