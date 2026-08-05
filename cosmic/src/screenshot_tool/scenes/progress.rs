use std::path::Path;

use cosmic_golden::HeadlessRenderer;
use read_flow_core::api::ReadingStatus;

use crate::app::Message;
use crate::page::DashboardMessage;
use crate::page::PageMessage;
use crate::screenshot_tool::app_harness::AppHarness;

pub(in crate::screenshot_tool) async fn render(sample_library: &Path) -> anyhow::Result<Vec<u8>> {
    let mut harness = AppHarness::new().await;

    // All 4 marked Reading so the dashboard's "Continue Reading" row shows
    // 4 real cards instead of padding out with invisible placeholders.
    let seeds = [
        ("leaves-of-grass.epub", ReadingStatus::Reading),
        ("the-time-machine.epub", ReadingStatus::Reading),
        ("twenty-thousand-leagues.epub", ReadingStatus::Reading),
        ("meditations.epub", ReadingStatus::Reading),
    ];
    let mut fixture_dirs = Vec::new();
    for (filename, status) in seeds {
        let (document, dir) = crate::test_support::scan_and_fetch_document(
            &harness.application_module,
            harness.document_provider(),
            sample_library.join(filename),
            filename,
        )
        .await;
        fixture_dirs.push(dir);
        for content in &document.contents {
            harness
                .document_provider()
                .update_reading_status(&content.fingerprint, status)
                .await?;
        }
    }

    // Dashboard is the default active page, but its initial LoadDashboard
    // (fired during AppHarness::new()) ran before any document existed —
    // resend it now that the library is seeded.
    harness
        .send(Message::Page(Box::new(PageMessage::Dashboard(
            DashboardMessage::LoadDashboard,
        ))))
        .await;

    let mut renderer = HeadlessRenderer::with_theme(super::super::theme());
    Ok(harness.render_rgba(&mut renderer))
}
