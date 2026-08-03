use std::path::Path;

use cosmic::Theme;
use cosmic_golden::HeadlessRenderer;
use read_flow_core::api::ReadingStatus;

use crate::page::Page as _;
use crate::page::dashboard::DashboardPage;

pub(in crate::screenshot_tool) async fn render(sample_library: &Path) -> anyhow::Result<Vec<u8>> {
    let (application_module, document_provider, _db_dir) =
        crate::test_support::document_provider().await;

    let seeds = [
        ("leaves-of-grass.epub", ReadingStatus::Unread),
        ("the-time-machine.epub", ReadingStatus::Reading),
        ("twenty-thousand-leagues.epub", ReadingStatus::Read),
        ("meditations.epub", ReadingStatus::Reading),
    ];
    let mut fixture_dirs = Vec::new();
    for (filename, status) in seeds {
        let (document, dir) = crate::test_support::scan_and_fetch_document(
            &application_module,
            &document_provider,
            sample_library.join(filename),
            filename,
        )
        .await;
        fixture_dirs.push(dir);
        for content in &document.contents {
            document_provider
                .update_reading_status(&content.fingerprint, status)
                .await?;
        }
    }

    let (mut page, init_task) = DashboardPage::new(document_provider);
    // `DashboardPage::new`'s init task just yields `LoadDashboard`; replaying
    // it drives the real async stats computation, whose own output message
    // (`Loaded`/similar) must be replayed too before `view()` shows anything
    // but the loading placeholder.
    for message in crate::test_support::drain(init_task).await {
        let follow_up = page.update(message);
        for message in crate::test_support::drain(follow_up).await {
            let _ = page.update(message);
        }
    }

    let element = page.view();
    let mut renderer = HeadlessRenderer::with_theme(Theme::dark());
    Ok(renderer.render(element, super::super::WIDTH, super::super::HEIGHT))
}
