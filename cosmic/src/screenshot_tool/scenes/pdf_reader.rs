use std::path::Path;

use cosmic::Theme;
use cosmic_golden::HeadlessRenderer;

use crate::page::Page as _;
use crate::page::mu_pdf_viewer::MuPdfViewer;
use crate::page::mu_pdf_viewer::MuPdfViewerMessage;

pub(in crate::screenshot_tool) async fn render(sample_library: &Path) -> anyhow::Result<Vec<u8>> {
    let (application_module, document_provider, _db_dir) =
        crate::test_support::document_provider().await;

    let (document, _fixture_dir) = crate::test_support::scan_and_fetch_document(
        &application_module,
        &document_provider,
        sample_library.join("jekyll-and-hyde.pdf"),
        "jekyll-and-hyde.pdf",
    )
    .await;

    let (mut viewer, init_task) = MuPdfViewer::new(document, document_provider);
    // `DisplayListReady`'s handler returns a further task (SVG/raster
    // generation for the visible page) that must itself be drained and
    // replayed — otherwise the page renders as a blank placeholder with no
    // content, only chrome (thumbnails, page counter).
    for message in crate::test_support::drain(init_task).await {
        if !matches!(message, MuPdfViewerMessage::Out(_)) {
            let follow_up = viewer.update(message);
            for message in crate::test_support::drain(follow_up).await {
                if !matches!(message, MuPdfViewerMessage::Out(_)) {
                    let _ = viewer.update(message);
                }
            }
        }
    }

    let mut renderer = HeadlessRenderer::with_theme(Theme::dark());

    // `viewport_size` is only set as a layout side effect inside the page's
    // `responsive(...)` closure, which only runs during an actual render
    // pass — not when `.view()` is merely called to build the Element tree.
    // So at drain time above, the page still assumed a single-pane layout
    // (viewport_size was still (0.0, 0.0)) and only rendered page 0. This
    // first render establishes the real (wide) viewport size; re-selecting
    // the active page then re-triggers rendering for whichever pages are
    // actually visible at that width (e.g. both pages of a dual-pane
    // spread), which we drain and replay before the real, final render.
    let _ = renderer.render(viewer.view(), super::super::WIDTH, super::super::HEIGHT);
    let follow_up = viewer.update(MuPdfViewerMessage::SelectPage(0));
    for message in crate::test_support::drain(follow_up).await {
        if !matches!(message, MuPdfViewerMessage::Out(_)) {
            let _ = viewer.update(message);
        }
    }

    let element = viewer.view();
    Ok(renderer.render(element, super::super::WIDTH, super::super::HEIGHT))
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
