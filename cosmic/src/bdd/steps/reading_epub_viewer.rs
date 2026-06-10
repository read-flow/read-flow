//! Steps for `features/reading_epub_viewer.feature`.
//!
//! The `Given a document has been added to the library` step is shared with
//! `documents_list` — it lives in that module.
use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

#[when("I open the document for reading")]
async fn open_for_reading(world: &mut BddWorld) {
    world.reading_open = true;
}

#[then("the EPUB content is displayed")]
async fn epub_content_displayed(world: &mut BddWorld) {
    let doc_api_guid = world
        .current_document_api_guid
        .as_deref()
        .expect("document must be seeded before opening for reading");
    let opened = world.driver.epub_opens_successfully(doc_api_guid).await;
    assert!(
        opened,
        "EPUB could not be opened for document {doc_api_guid}"
    );
}
