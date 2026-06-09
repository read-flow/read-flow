//! Steps for `features/documents_batch_tag.feature`.
//!
//! The `Then "…" appears in the document's tag list` step is defined in
//! `tags_add` and shared here.
use cucumber::when;

use crate::bdd::world::BddWorld;

#[when(regex = r#"^I batch-add tag "([^"]+)" to the selected documents$"#)]
async fn batch_add_tag(world: &mut BddWorld, tag: String) {
    let doc_api_guid = world
        .current_document_api_guid
        .as_ref()
        .expect("no current document — seed step must run first")
        .clone();
    world.driver.batch_add_tag(&doc_api_guid, &tag).await;
}
