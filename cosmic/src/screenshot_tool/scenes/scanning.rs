use std::path::Path;

use cosmic::Theme;
use cosmic_golden::HeadlessRenderer;

use crate::component::documents::DocumentsMessage;
use crate::config::Config;
use crate::page::DocumentListMessage;
use crate::page::PageMessage;
use crate::page::PageSelector;
use crate::page::Pages;

pub(in crate::screenshot_tool) async fn render(sample_library: &Path) -> anyhow::Result<Vec<u8>> {
    let (application_module, document_provider, _db_dir) =
        crate::test_support::document_provider().await;

    // Same book, different format/hash — two distinct rows, real candidates
    // for the merge feature. NOT the byte-identical `(copy).epub`: the
    // scanner auto-merges byte-identical content into one Document with
    // multiple sources, so that pair would collapse into a single row with
    // nothing to select.
    let (epub_doc, _epub_dir) = crate::test_support::scan_and_fetch_document(
        &application_module,
        &document_provider,
        sample_library.join("pride-and-prejudice.epub"),
        "pride-and-prejudice.epub",
    )
    .await;
    let (pdf_doc, _pdf_dir) = crate::test_support::scan_and_fetch_document(
        &application_module,
        &document_provider,
        sample_library.join("pride-and-prejudice.pdf"),
        "pride-and-prejudice.pdf",
    )
    .await;

    let (mut pages, init_task) = Pages::new(
        application_module,
        document_provider,
        Config::default(),
        crate::logging::init(),
    );
    // `Pages::new`'s init task includes `DocumentList`'s `LoadArchive`
    // message, which itself must be replayed into `update()` to trigger (and
    // then drain) the real async document fetch — otherwise the list stays
    // on its initial "Loading" placeholder.
    for message in crate::test_support::drain(init_task).await {
        if !matches!(message, PageMessage::Out(_)) {
            let follow_up = pages.update(message);
            for message in crate::test_support::drain(follow_up).await {
                if !matches!(message, PageMessage::Out(_)) {
                    let _ = pages.update(message);
                }
            }
        }
    }

    for document in [epub_doc, pdf_doc] {
        let messages = crate::test_support::drain(pages.update(PageMessage::Documents(
            DocumentListMessage::DocumentsComponent(DocumentsMessage::ToggleDocumentSelected(
                document,
            )),
        )))
        .await;
        for message in messages {
            if !matches!(message, PageMessage::Out(_)) {
                let _ = pages.update(message);
            }
        }
    }

    let element = pages.view(&PageSelector::Documents);
    let mut renderer = HeadlessRenderer::with_theme(Theme::dark());
    Ok(renderer.render(element, super::super::WIDTH, super::super::HEIGHT))
}
