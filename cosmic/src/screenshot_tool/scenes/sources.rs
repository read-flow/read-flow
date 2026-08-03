use std::path::Path;

use cosmic::Theme;
use cosmic_golden::HeadlessRenderer;
use read_flow_core::db::models::Remote;

use crate::component::provided_state::ProvidedStateMessage;
use crate::config::Config;
use crate::page::Page as _;
use crate::page::PreferencesMessage;
use crate::page::PreferencesPage;
use crate::page::PreferencesSection;

pub(in crate::screenshot_tool) async fn render(_sample_library: &Path) -> anyhow::Result<Vec<u8>> {
    let (application_module, document_provider, _db_dir) =
        crate::test_support::document_provider().await;

    let (mut page, init_task) =
        PreferencesPage::new(application_module, Config::default(), document_provider);
    crate::test_support::drain(init_task).await;

    let remotes = vec![
        Remote {
            id: 1,
            base_url: "https://library.example.com".to_string(),
            order: 0,
            passphrase: String::new(),
            user_id: "reader".to_string(),
        },
        Remote {
            id: 2,
            base_url: "https://office.example.com".to_string(),
            order: 1,
            passphrase: String::new(),
            user_id: "reader".to_string(),
        },
    ];

    // Deliberately DISCARD the returned Task: `Remotes(Loaded(..))`'s update
    // arm auto-schedules a real `CheckSourceStatus` HTTP health-check per
    // remote. Since nothing in this tool polls that task, it never runs —
    // Rust futures do nothing until polled, and `drain()` is the only poller
    // this tool has. We force "reachable" ourselves instead, synchronously.
    let _ = page.update(PreferencesMessage::Remotes(ProvidedStateMessage::Loaded(
        remotes,
    )));
    let _ = page.update(PreferencesMessage::SetSourceStatus(1, true));
    let _ = page.update(PreferencesMessage::SetSourceStatus(2, true));
    let _ = page.update(PreferencesMessage::SectionChanged(
        PreferencesSection::Sources,
    ));

    let element = page.view();
    let mut renderer = HeadlessRenderer::with_theme(Theme::dark());
    Ok(renderer.render(element, super::super::WIDTH, super::super::HEIGHT))
}
