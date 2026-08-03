use std::path::Path;

use cosmic::Theme;
use cosmic_golden::HeadlessRenderer;

use crate::component::tag_editor::TagEditorMessage;
use crate::page::Page as _;
use crate::page::document_details::DocumentDetails;
use crate::page::document_details::DocumentDetailsMessage;

pub(in crate::screenshot_tool) async fn render(sample_library: &Path) -> anyhow::Result<Vec<u8>> {
    let (application_module, document_provider, _db_dir) =
        crate::test_support::document_provider().await;

    let (document, _fixture_dir) = crate::test_support::scan_and_fetch_document(
        &application_module,
        &document_provider,
        sample_library.join("meditations.epub"),
        "meditations.epub",
    )
    .await;

    let (mut page, init_task) =
        DocumentDetails::new(document, document_provider, application_module);
    crate::test_support::drain(init_task).await;

    let tags = vec![
        "philosophy".to_string(),
        "classic".to_string(),
        "public-domain".to_string(),
        "to-read".to_string(),
    ];
    let _ = page.update(DocumentDetailsMessage::TagEditor(
        TagEditorMessage::SetTags(tags),
    ));

    let element = page.view();
    let mut renderer = HeadlessRenderer::with_theme(Theme::dark());
    Ok(renderer.render(element, super::super::WIDTH, super::super::HEIGHT))
}
