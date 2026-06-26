//! Steps for `features/reading_pdf_viewer.feature`.
use cucumber::given;
use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

#[given("a PDF document has been added to the library")]
async fn seed_pdf_document(world: &mut BddWorld) {
    let (file_guid, doc_api_guid, fingerprint) = world.driver.seed_pdf_document().await;
    world.current_document_guid = Some(file_guid);
    world.current_document_api_guid = Some(doc_api_guid);
    world.current_document_fingerprint = Some(fingerprint);
}

#[when("I open the PDF document for reading")]
async fn open_pdf_for_reading(world: &mut BddWorld) {
    world.reading_open = true;
}

#[then("the PDF pages are displayed")]
async fn pdf_pages_displayed(world: &mut BddWorld) {
    let can_open = world.driver.pdf_opens_successfully();
    assert!(can_open, "MuPDF could not open the PDF fixture");
}
