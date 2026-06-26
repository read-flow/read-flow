//! Steps for `features/documents_format_picker.feature`.
use cucumber::given;
use cucumber::then;

use crate::bdd::world::BddWorld;

// `Given a read-flow server is running…` is in common.rs.

#[given("an EPUB and a PDF document have been added and merged")]
async fn seed_merged_multiformat(world: &mut BddWorld) {
    let winner_guid = world.driver.seed_merged_multiformat_document().await;
    world.current_document_api_guid = Some(winner_guid);
}

#[then("multiple format choices are available for the merged document")]
async fn multiple_formats_available(world: &mut BddWorld) {
    let guid = world
        .current_document_api_guid
        .as_deref()
        .expect("merged document guid must be set");
    let has_multiple = world.driver.document_has_multiple_formats(guid).await;
    assert!(
        has_multiple,
        "expected merged document {guid} to have multiple format choices"
    );
}
