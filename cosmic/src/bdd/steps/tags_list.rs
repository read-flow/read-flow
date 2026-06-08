//! Steps for `features/tags_list.feature` — the first scenario needing a
//! seeded document. Reuses the `Given a read-flow server is running...` step
//! from `remotes_status`.
use cucumber::given;
use cucumber::then;

use crate::bdd::world::BddWorld;

#[given(regex = "^a document tagged \"([^\"]+)\" has been added to the library$")]
async fn seed_tagged_document(world: &mut BddWorld, tag: String) {
    world.driver.seed_tagged_document(&tag).await;
}

#[then(regex = "^\"([^\"]+)\" appears in the library's list of tags$")]
async fn tag_is_listed(world: &mut BddWorld, tag: String) {
    assert!(
        world.driver.tag_is_listed(&tag).await,
        "expected {tag:?} to appear in the library's list of tags"
    );
}
