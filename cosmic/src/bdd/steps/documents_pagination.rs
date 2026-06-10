//! Steps for `features/documents_pagination.feature`.
use cucumber::then;

use crate::bdd::world::BddWorld;

// `Given a read-flow server is running…` and `And a document has been added to the library`
// are in common.rs and documents_list.rs respectively.

#[then(regex = r#"^"([^"]+)" appears on the first page of the document list$"#)]
async fn document_on_first_page(world: &mut BddWorld, title: String) {
    let found = world.driver.document_on_first_page(&title).await;
    assert!(
        found,
        "expected document \"{title}\" to appear on the first page of the document list"
    );
}
