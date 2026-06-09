//! Steps for `features/tags_list.feature` — the first scenario needing a
//! seeded document. Reuses the `Given a read-flow server is running...` step
//! from `remotes_status`.
use cucumber::given;
use cucumber::then;

use crate::bdd::world::BddWorld;

#[given(regex = "^a document tagged \"([^\"]+)\" has been added to the library$")]
async fn seed_tagged_document(world: &mut BddWorld, tag: String) {
    let (file_guid, doc_api_guid, fingerprint) = world.driver.seed_tagged_document(&tag).await;
    world.current_document_guid = Some(file_guid);
    world.current_document_fingerprint = Some(fingerprint);
    world.current_document_api_guid = Some(doc_api_guid);
}

#[then(regex = "^\"([^\"]+)\" appears in the library's list of tags$")]
async fn tag_is_listed(world: &mut BddWorld, tag: String) {
    assert!(
        world.driver.tag_is_listed(&tag).await,
        "expected {tag:?} to appear in the library's list of tags"
    );
}
