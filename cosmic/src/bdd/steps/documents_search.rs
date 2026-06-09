//! Steps for `features/documents_search.feature`.
use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

#[when(regex = r#"^I search for "([^"]+)"$"#)]
async fn search_for(world: &mut BddWorld, query: String) {
    world.search_query = Some(query);
}

#[then(regex = r#"^"([^"]+)" appears in the search results$"#)]
async fn appears_in_search_results(world: &mut BddWorld, title: String) {
    let query = world
        .search_query
        .as_deref()
        .expect("search_for step must run first");
    let found = world.driver.search_returns_document(query, &title).await;
    assert!(
        found,
        "expected \"{title}\" in search results for query {query:?}"
    );
}
