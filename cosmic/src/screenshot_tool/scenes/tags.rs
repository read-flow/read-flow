use std::path::Path;

use cosmic_golden::HeadlessRenderer;

use crate::app::Message;
use crate::component::tag_editor::TagEditorMessage;
use crate::page::PageMessage;
use crate::page::document_details::DocumentDetailsMessage;
use crate::screenshot_tool::app_harness::AppHarness;

pub(in crate::screenshot_tool) async fn render(sample_library: &Path) -> anyhow::Result<Vec<u8>> {
    let mut harness = AppHarness::new().await;

    let (document, _fixture_dir) = crate::test_support::scan_and_fetch_document(
        &harness.application_module,
        harness.document_provider(),
        sample_library.join("meditations.epub"),
        "meditations.epub",
    )
    .await;
    let document_guid = document.document_guid.clone();

    // Opening a document's details auto-registers and auto-activates that
    // page, keyed by `document_guid` (unlike the reader viewers, which key
    // by content fingerprint).
    harness
        .send(Message::Page(Box::new(PageMessage::OpenDocumentDetails(
            document,
        ))))
        .await;

    let tags = vec![
        "philosophy".to_string(),
        "classic".to_string(),
        "public-domain".to_string(),
        "to-read".to_string(),
    ];
    harness
        .send(Message::Page(Box::new(PageMessage::DocumentDetails(
            document_guid,
            DocumentDetailsMessage::TagEditor(TagEditorMessage::SetTags(tags)),
        ))))
        .await;

    let mut renderer = HeadlessRenderer::with_theme(super::super::theme(sample_library)?);
    Ok(harness.render_rgba(&mut renderer))
}
