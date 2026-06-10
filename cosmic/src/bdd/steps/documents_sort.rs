//! Steps for `features/documents_sort.feature`.
use cucumber::given;
use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

#[given("two documents have been added to the library")]
async fn seed_two_documents(world: &mut BddWorld) {
    let (file_guid, doc_api_guid, fingerprint) = world.driver.seed_document().await;
    world.current_document_guid = Some(file_guid);
    world.current_document_api_guid = Some(doc_api_guid);
    world.current_document_fingerprint = Some(fingerprint);
    let (_, second_api_guid, _) = world.driver.seed_second_document().await;
    world.second_document_api_guid = Some(second_api_guid);
}

#[when("I sort the documents by title ascending")]
async fn sort_by_title_ascending(world: &mut BddWorld) {
    world.sort_ascending = true;
}

#[then(regex = r#"^"([^"]+)" appears before "([^"]+)" in the list$"#)]
async fn first_appears_before_second(world: &mut BddWorld, first: String, second: String) {
    let titles = world.driver.sorted_document_titles_ascending().await;
    let pos_first = titles
        .iter()
        .position(|t| t == &first)
        .unwrap_or_else(|| panic!("{first:?} not found in sorted list: {titles:?}"));
    let pos_second = titles
        .iter()
        .position(|t| t == &second)
        .unwrap_or_else(|| panic!("{second:?} not found in sorted list: {titles:?}"));
    assert!(
        pos_first < pos_second,
        "expected {first:?} (pos {pos_first}) before {second:?} (pos {pos_second})"
    );
}
