//! Steps for `features/documents_filter_by_tag.feature`.
//!
//! The `Then "…" appears in the filtered results` step is shared with
//! `documents_filter_by_status` — it lives in that module and dispatches on
//! whichever filter (`status_filter` / `tag_filter`) is set in the world.
use cucumber::when;

use crate::bdd::world::BddWorld;

#[when(regex = r#"^I filter by tag "([^"]+)"$"#)]
async fn filter_by_tag(world: &mut BddWorld, tag: String) {
    world.tag_filter = Some(tag);
}
