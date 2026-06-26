//! Steps for `features/reading_status.feature`.
use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

#[when(regex = r#"^I set the document's reading status to "([^"]+)"$"#)]
async fn set_reading_status(world: &mut BddWorld, status: String) {
    let guid = world
        .current_document_guid
        .as_ref()
        .expect("no current document — seed step must run first")
        .clone();
    world.driver.set_reading_status(&guid, &status).await;
}

#[then(regex = r#"^the document's reading status is "([^"]+)"$"#)]
async fn reading_status_is(world: &mut BddWorld, expected: String) {
    let guid = world
        .current_document_guid
        .as_ref()
        .expect("no current document — seed step must run first");
    let actual = world.driver.get_reading_status(guid).await;
    assert_eq!(actual, expected, "reading status mismatch");
}
