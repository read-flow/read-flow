//! Steps for `features/documents_filter_by_source.feature`.
use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

// `Given a read-flow server is running…` and `And a document has been added to the library`
// are in common.rs and documents_list.rs respectively.

#[when(regex = r#"^I filter documents by source "([^"]+)"$"#)]
async fn filter_by_source(world: &mut BddWorld, source_name: String) {
    world.source_filter = Some(source_name);
}

#[then(regex = r#"^"([^"]+)" appears in the filtered document list$"#)]
async fn document_appears_in_filtered_list(world: &mut BddWorld, title: String) {
    let source_name = world
        .source_filter
        .as_deref()
        .expect("source filter must be set before checking results");
    let found = world
        .driver
        .filter_by_source_returns_document(source_name, &title)
        .await;
    assert!(
        found,
        "expected document \"{title}\" to appear when filtering by source \"{source_name}\""
    );
}
