//! Steps for `features/documents_change_thumbnail.feature`.
//!
//! `Given a read-flow server is running…` is in `remotes_status.rs`.
//! `Given a PDF document has been added to the library` is in `reading_pdf_viewer.rs`.
//! `Then a cover image is returned` is in `documents_cover_display.rs`.
use cucumber::when;

use crate::bdd::world::BddWorld;

#[when("I set the document's thumbnail to its first page, untrimmed")]
async fn set_thumbnail_to_first_page(world: &mut BddWorld) {
    let file_guid = world
        .current_document_guid
        .clone()
        .expect("PDF document must be seeded before setting its thumbnail");
    world
        .driver
        .set_pdf_page_thumbnail(
            &file_guid,
            0,
            false,
            0,
            read_flow_core::scan::cover::TrimMargins::default(),
        )
        .await;
}
