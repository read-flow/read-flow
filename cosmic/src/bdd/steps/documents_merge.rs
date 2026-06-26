//! Steps for `features/documents_merge.feature`.
//!
//! The `Given two documents have been added to the library` step is shared
//! with `documents_sort` — it lives in that module.
use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

#[when("I merge the two documents")]
async fn merge_two_documents(world: &mut BddWorld) {
    let winner = world
        .current_document_api_guid
        .as_deref()
        .expect("first document must be seeded first")
        .to_string();
    let loser = world
        .second_document_api_guid
        .as_deref()
        .expect("second document must be seeded first")
        .to_string();
    world.driver.merge_documents(&winner, &loser).await;
}

#[then("only one document remains in the library")]
async fn one_document_remains(world: &mut BddWorld) {
    let count = world.driver.document_count().await;
    assert_eq!(count, 1, "expected 1 document after merge, got {count}");
}
