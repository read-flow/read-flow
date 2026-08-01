use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

#[when(regex = r#"^I set the reading progress to (\d+)% at position "([^"]+)"$"#)]
async fn set_reading_progress(world: &mut BddWorld, percentage: u64, position: String) {
    let fingerprint = world
        .current_document_fingerprint
        .clone()
        .expect("seed step must run first");
    world
        .driver
        .set_reading_progress(&fingerprint, &position, percentage as f64 / 100.0)
        .await;
}

#[then(regex = r#"^the reading progress is (\d+)% at "([^"]+)"$"#)]
async fn reading_progress_is(world: &mut BddWorld, expected_pct: u64, expected_pos: String) {
    let fingerprint = world
        .current_document_fingerprint
        .as_deref()
        .expect("seed step must run first");
    let (position, percentage) = world.driver.get_reading_progress(fingerprint).await;
    assert_eq!(
        position, expected_pos,
        "expected position {expected_pos:?}, got {position:?}"
    );
    let expected_float = expected_pct as f64 / 100.0;
    assert!(
        (percentage - expected_float).abs() < 1e-9,
        "expected percentage {expected_float}, got {percentage}"
    );
}

#[when("I open the EPUB and PDF documents for reading")]
async fn open_epub_and_pdf_for_reading(world: &mut BddWorld) {
    let fingerprints = world.driver.open_epub_and_pdf_for_reading().await;
    world.open_reading_fingerprints = Some(fingerprints);
}

#[when("I close the application")]
async fn close_the_application(world: &mut BddWorld) {
    world.driver.close_application().await;
}

#[then("reading progress was saved for both open documents")]
async fn reading_progress_saved_for_both(world: &mut BddWorld) {
    let (epub_fingerprint, pdf_fingerprint) = world
        .open_reading_fingerprints
        .clone()
        .expect("open step must run first");

    let (epub_position, epub_percentage) =
        world.driver.get_reading_progress(&epub_fingerprint).await;
    assert!(
        epub_position.contains("cfi"),
        "expected a CFI position, got {epub_position:?}"
    );
    assert!(epub_percentage > 0.0, "expected a nonzero percentage");

    let (pdf_position, pdf_percentage) = world.driver.get_reading_progress(&pdf_fingerprint).await;
    assert!(
        pdf_position.contains("\"page\":1"),
        "expected page 1, got {pdf_position:?}"
    );
    assert!(
        (pdf_percentage - 1.0).abs() < 1e-9,
        "expected 100% (single-page fixture), got {pdf_percentage}"
    );
}
