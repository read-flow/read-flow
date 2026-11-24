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

#[then("the standalone reader can open and resume a local EPUB")]
async fn standalone_epub_reopens_at_reading_position(_world: &mut BddWorld) {
    use std::path::Path;

    use cosmic_epub::reader::Reader;

    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../features/fixtures/sample.epub");
    let mut reader = Reader::open(&path, None).expect("open a local EPUB");
    assert!(!reader.toc().is_empty());
    reader.select_nav(0);
    let saved = reader.position().expect("the book has a reading position");
    let reopened = Reader::open(&path, Some(saved.clone())).expect("reopen the EPUB");
    let restored = reopened.position().expect("restore the reading position");
    assert_eq!(restored.chapter_href, saved.chapter_href);
    assert_eq!(restored.cfi, saved.cfi);
    assert_eq!(restored.cut, saved.cut);
}
