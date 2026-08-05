use std::path::Path;

use cosmic_golden::HeadlessRenderer;
use read_flow_core::db::models::Remote;

use crate::app::Message;
use crate::component::provided_state::ProvidedStateMessage;
use crate::page::PageMessage;
use crate::page::PageSelector;
use crate::page::PreferencesMessage;
use crate::page::PreferencesSection;
use crate::screenshot_tool::app_harness::AppHarness;

pub(in crate::screenshot_tool) async fn render(_sample_library: &Path) -> anyhow::Result<Vec<u8>> {
    let mut harness = AppHarness::new().await;
    harness
        .send(Message::ActivatePage(PageSelector::Preferences))
        .await;

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
    // remote. Since nothing polls that task, it never runs — Rust futures
    // do nothing until polled. We force "reachable" ourselves instead.
    harness.send_without_draining(Message::Page(Box::new(PageMessage::Preferences(
        PreferencesMessage::Remotes(ProvidedStateMessage::Loaded(remotes)),
    ))));
    harness
        .send(Message::Page(Box::new(PageMessage::Preferences(
            PreferencesMessage::SetSourceStatus(1, true),
        ))))
        .await;
    harness
        .send(Message::Page(Box::new(PageMessage::Preferences(
            PreferencesMessage::SetSourceStatus(2, true),
        ))))
        .await;
    harness
        .send(Message::Page(Box::new(PageMessage::Preferences(
            PreferencesMessage::SectionChanged(PreferencesSection::Sources),
        ))))
        .await;

    let mut renderer = HeadlessRenderer::with_theme(super::super::theme());
    Ok(harness.render_rgba(&mut renderer))
}
