use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

#[when("I view the document's details")]
async fn view_document_details(_world: &mut BddWorld) {
    // REST/Cosmic: "viewing details" is a GET — nothing to trigger.
    // The Then step retrieves the title directly via the driver.
}

#[then(regex = r#"^the document's title is "([^"]+)"$"#)]
async fn document_title_is(world: &mut BddWorld, expected: String) {
    let guid = world
        .current_document_api_guid
        .as_deref()
        .expect("seed step must run first");
    let actual = world.driver.get_document_title(guid).await;
    assert_eq!(
        actual, expected,
        "expected document title {expected:?}, got {actual:?}"
    );
}
