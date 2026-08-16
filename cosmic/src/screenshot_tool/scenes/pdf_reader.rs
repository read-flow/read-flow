use std::path::Path;

use cosmic_golden::HeadlessRenderer;

use crate::app::Message;
use crate::page::PageMessage;
use crate::page::mu_pdf_viewer::MuPdfViewerMessage;
use crate::screenshot_tool::app_harness::AppHarness;

pub(in crate::screenshot_tool) async fn render(sample_library: &Path) -> anyhow::Result<Vec<u8>> {
    let mut harness = AppHarness::new().await;

    let (document, _fixture_dir) = crate::test_support::scan_and_fetch_document(
        &harness.application_module,
        harness.document_provider(),
        sample_library.join("jekyll-and-hyde.pdf"),
        "jekyll-and-hyde.pdf",
    )
    .await;
    let fingerprint = document.contents[0].fingerprint.clone();

    harness
        .send(Message::Page(Box::new(PageMessage::OpenDocument(document))))
        .await;

    let mut renderer = HeadlessRenderer::with_theme(super::super::theme(sample_library)?);

    // `viewport_size` is only set as a layout side effect inside the PDF
    // page's `responsive(...)` closure, which only runs during an actual
    // render pass — not when messages are replayed. So at this point the
    // page still assumed a single-pane layout and only rendered page 0.
    // This first render establishes the real (wide) viewport size;
    // re-selecting the active page then re-triggers rendering for whichever
    // pages are actually visible at that width (e.g. both pages of a
    // dual-page spread), which we drain and replay before the real, final
    // render.
    let _ = harness.render_rgba(&mut renderer);
    harness
        .send(Message::Page(Box::new(PageMessage::MuPdfViewer(
            fingerprint,
            MuPdfViewerMessage::SelectPage(0),
        ))))
        .await;

    Ok(harness.render_rgba(&mut renderer))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Guards against a future mupdf/library upgrade silently breaking
    /// rendering and producing a blank marketing screenshot.
    #[tokio::test]
    async fn pdf_reader_scene_renders_non_blank_pixels() {
        let sample_library = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../read-flow.github.io/assets/sample-library");
        let rgba = render(&sample_library)
            .await
            .expect("render pdf reader scene");

        assert_eq!(
            rgba.len(),
            (super::super::super::WIDTH * super::super::super::HEIGHT * 4) as usize
        );
        let all_same = rgba.chunks_exact(4).all(|p| p == &rgba[0..4]);
        assert!(
            !all_same,
            "rendered PDF page is a single solid color — rendering likely broke"
        );
    }
}
