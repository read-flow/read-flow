//! Steps for `features/documents_list.feature`. Reuses the
//! `Given a read-flow server is running...` step from `remotes_status`.
use cucumber::given;
use cucumber::then;

use crate::bdd::world::BddWorld;

#[given("a document has been added to the library")]
async fn seed_document(world: &mut BddWorld) {
    world.driver.seed_document().await;
}

#[then(regex = "^\"([^\"]+)\" appears in the library's list of documents$")]
async fn document_is_listed(world: &mut BddWorld, title: String) {
    assert!(
        world.driver.document_is_listed(&title).await,
        "expected {title:?} to appear in the library's list of documents"
    );
}
