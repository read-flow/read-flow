use std::path::Path;

use cosmic::Theme;
use cosmic_golden::HeadlessRenderer;

use crate::app::Message;
use crate::page::PageMessage;
use crate::page::epub_viewer::EpubViewerMessage;
use crate::screenshot_tool::app_harness::AppHarness;

pub(in crate::screenshot_tool) async fn render(sample_library: &Path) -> anyhow::Result<Vec<u8>> {
    let mut harness = AppHarness::new().await;

    let (document, _fixture_dir) = crate::test_support::scan_and_fetch_document(
        &harness.application_module,
        harness.document_provider(),
        sample_library.join("the-time-machine.epub"),
        "the-time-machine.epub",
    )
    .await;
    let fingerprint = document.contents[0].fingerprint.clone();

    // Opening a document routes to the EPUB or PDF viewer based on type,
    // and auto-activates that page, keyed by content fingerprint.
    harness
        .send(Message::Page(Box::new(PageMessage::OpenDocument(document))))
        .await;

    // Chapter 0 is this book's table of contents; jump to the first real
    // chapter so the screenshot shows readable prose, per the spec.
    harness
        .send(Message::Page(Box::new(PageMessage::EpubViewer(
            fingerprint,
            EpubViewerMessage::SelectChapter(2),
        ))))
        .await;

    let mut renderer = HeadlessRenderer::with_theme(Theme::dark());
    Ok(harness.render_rgba(&mut renderer))
}
