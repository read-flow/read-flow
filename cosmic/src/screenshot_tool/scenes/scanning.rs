use std::path::Path;

use cosmic_golden::HeadlessRenderer;

use crate::app::Message;
use crate::component::documents::DocumentsMessage;
use crate::page::DocumentListMessage;
use crate::page::PageMessage;
use crate::page::PageSelector;
use crate::screenshot_tool::app_harness::AppHarness;

pub(in crate::screenshot_tool) async fn render(sample_library: &Path) -> anyhow::Result<Vec<u8>> {
    let mut harness = AppHarness::new().await;

    // Same book, different format/hash — two distinct rows, real candidates
    // for the merge feature. NOT the byte-identical `(copy).epub`: the
    // scanner auto-merges byte-identical content into one Document with
    // multiple sources, so that pair would collapse into a single row with
    // nothing to select.
    let (epub_doc, _epub_dir) = crate::test_support::scan_and_fetch_document(
        &harness.application_module,
        harness.document_provider(),
        sample_library.join("pride-and-prejudice.epub"),
        "pride-and-prejudice.epub",
    )
    .await;
    let (pdf_doc, _pdf_dir) = crate::test_support::scan_and_fetch_document(
        &harness.application_module,
        harness.document_provider(),
        sample_library.join("pride-and-prejudice.pdf"),
        "pride-and-prejudice.pdf",
    )
    .await;

    // DocumentList's initial LoadArchive (fired during AppHarness::new())
    // ran before any document existed — resend it now that the library is
    // seeded, same as the dashboard scene.
    harness
        .send(Message::Page(Box::new(PageMessage::Documents(
            DocumentListMessage::LoadArchive,
        ))))
        .await;
    harness
        .send(Message::ActivatePage(PageSelector::Documents))
        .await;

    for document in [epub_doc, pdf_doc] {
        harness
            .send(Message::Page(Box::new(PageMessage::Documents(
                DocumentListMessage::DocumentsComponent(DocumentsMessage::ToggleDocumentSelected(
                    document,
                )),
            ))))
            .await;
    }

    let mut renderer = HeadlessRenderer::with_theme(super::super::theme());
    Ok(harness.render_rgba(&mut renderer))
}
