//! Steps for `features/documents_select_cover.feature`.
use cucumber::when;

use crate::bdd::world::BddWorld;

// `Given a read-flow server is running…` is in common.rs.
// `And a document with a cover image has been added to the library` is in
//   documents_cover_display.rs.
// `Then a cover image is returned` is in documents_cover_display.rs.

#[when("I set the document's cover to its file's cover image")]
async fn set_cover_fingerprint(world: &mut BddWorld) {
    let doc_api_guid = world
        .current_document_api_guid
        .clone()
        .expect("doc_api_guid must be set (seed a cover document first)");
    let fingerprint = world
        .current_document_fingerprint
        .clone()
        .expect("fingerprint must be set (seed a cover document first)");
    world
        .driver
        .set_document_cover_fingerprint(&doc_api_guid, &fingerprint)
        .await;
}
