use std::path::Path;

use cosmic::Theme;
use cosmic_golden::HeadlessRenderer;

use crate::page::Page as _;
use crate::page::epub_viewer::EpubViewer;
use crate::page::epub_viewer::EpubViewerMessage;

pub(in crate::screenshot_tool) async fn render(sample_library: &Path) -> anyhow::Result<Vec<u8>> {
    let (application_module, document_provider, _db_dir) =
        crate::test_support::document_provider().await;

    let (document, _fixture_dir) = crate::test_support::scan_and_fetch_document(
        &application_module,
        &document_provider,
        sample_library.join("the-time-machine.epub"),
        "the-time-machine.epub",
    )
    .await;

    let (mut viewer, init_task) = EpubViewer::new(document, document_provider);
    let messages = crate::test_support::drain(init_task).await;
    for message in messages {
        if !matches!(message, EpubViewerMessage::Out(_)) {
            let _ = viewer.update(message);
        }
    }
    // Chapter 0 is this book's table of contents; jump to the first real
    // chapter so the screenshot shows readable prose, per the spec.
    let _ = viewer.update(EpubViewerMessage::SelectChapter(2));

    let element = viewer.view();
    let mut renderer = HeadlessRenderer::with_theme(Theme::dark());
    Ok(renderer.render(element, super::super::WIDTH, super::super::HEIGHT))
}
