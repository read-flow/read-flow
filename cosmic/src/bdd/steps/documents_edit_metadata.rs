// `Then "the document's title is {string}"` is defined in `documents_detail_view`.
use cucumber::when;

use crate::bdd::world::BddWorld;

#[when(regex = r#"^I set the document's title to "([^"]+)"$"#)]
async fn set_document_title(world: &mut BddWorld, title: String) {
    let guid = world
        .current_document_api_guid
        .as_deref()
        .expect("seed step must run first");
    world.driver.set_document_title(guid, &title).await;
}
