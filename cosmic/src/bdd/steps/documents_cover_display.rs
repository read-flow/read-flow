//! Steps for `features/documents_cover_display.feature`.
use cucumber::given;
use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

#[given("a document with a cover image has been added to the library")]
async fn seed_cover_document(world: &mut BddWorld) {
    let (file_guid, doc_api_guid, fingerprint) = world.driver.seed_cover_document().await;
    world.current_document_guid = Some(file_guid);
    world.current_document_api_guid = Some(doc_api_guid);
    world.current_document_fingerprint = Some(fingerprint);
}

#[when("I request the document's cover")]
async fn request_cover(_world: &mut BddWorld) {
    // Marker step — the actual assertion is in the Then step.
}

#[then("a cover image is returned")]
async fn cover_image_is_returned(world: &mut BddWorld) {
    let doc_api_guid = world
        .current_document_api_guid
        .as_deref()
        .expect("document must be seeded before checking cover");
    let has_cover = world.driver.document_has_cover(doc_api_guid).await;
    assert!(
        has_cover,
        "expected a cover image for document {doc_api_guid}"
    );
}
