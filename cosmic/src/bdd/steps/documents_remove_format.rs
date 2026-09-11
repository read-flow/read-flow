//! Steps for `features/documents_remove_format.feature`.
//!
//! The `Given two documents have been added to the library` and
//! `When I merge the two documents` steps are shared with `documents_merge` /
//! `documents_sort` — they live in those modules.
use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

#[when("I remove the second document's format")]
async fn remove_second_document_format(world: &mut BddWorld) {
    let doc_guid = world
        .current_document_api_guid
        .as_deref()
        .expect("first document must be seeded first")
        .to_string();
    let fingerprint = world
        .second_document_fingerprint
        .as_deref()
        .expect("second document must be seeded first")
        .to_string();
    world
        .driver
        .remove_content_from_document(&doc_guid, &fingerprint)
        .await;
}

#[when("I remove the current document's format")]
async fn remove_current_document_format(world: &mut BddWorld) {
    let doc_guid = world
        .current_document_api_guid
        .clone()
        .expect("document must be seeded first");
    let fingerprint = world
        .current_document_fingerprint
        .clone()
        .expect("document must be seeded first");
    world
        .driver
        .remove_content_from_document(&doc_guid, &fingerprint)
        .await;
}

#[then("the merged document has only its original format")]
async fn merged_document_keeps_original_format(world: &mut BddWorld) {
    let doc_guid = world
        .current_document_api_guid
        .as_deref()
        .expect("first document must be seeded first");
    let count = world.driver.format_count_for_document(doc_guid).await;
    assert!(count == 1, "expected 1 format after removal, got {count}");
}

#[then("the removed format's files are gone from the file index")]
async fn removed_format_files_are_gone(world: &mut BddWorld) {
    let fingerprint = world
        .second_document_fingerprint
        .as_deref()
        .expect("second document must be seeded first");
    assert!(
        !world.driver.fingerprint_has_files(fingerprint).await,
        "expected no files to remain for the removed format"
    );
}

#[then("the document no longer exists in the library")]
async fn document_no_longer_exists(world: &mut BddWorld) {
    let doc_guid = world
        .current_document_api_guid
        .as_deref()
        .expect("document must be seeded first");
    assert!(
        !world.driver.document_is_listed_by_guid(doc_guid).await,
        "expected the document to be deleted after removing its last format"
    );
}
