//! Steps for `features/documents_filter_by_status.feature`.
use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

#[when(regex = r#"^I filter by reading status "([^"]+)"$"#)]
async fn filter_by_status(world: &mut BddWorld, status: String) {
    world.status_filter = Some(status);
}

#[then(regex = r#"^"([^"]+)" appears in the filtered results$"#)]
async fn appears_in_filtered_results(world: &mut BddWorld, title: String) {
    if let Some(status) = world.status_filter.clone() {
        let found = world
            .driver
            .filter_by_status_returns_document(&status, &title)
            .await;
        assert!(
            found,
            "expected \"{title}\" in results filtered by status {status:?}"
        );
    } else if let Some(tag) = world.tag_filter.clone() {
        let found = world
            .driver
            .filter_by_tag_returns_document(&tag, &title)
            .await;
        assert!(
            found,
            "expected \"{title}\" in results filtered by tag {tag:?}"
        );
    } else {
        panic!("no filter set — run a filter step before asserting filtered results");
    }
}
