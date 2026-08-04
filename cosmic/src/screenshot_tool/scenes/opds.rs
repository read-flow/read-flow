use std::path::Path;

use cosmic::Theme;
use cosmic_golden::HeadlessRenderer;
use read_flow_core::online_library::DownloadFormat;
use read_flow_core::online_library::OnlineBook;

use crate::app::Message;
use crate::page::OnlineLibraryMessage;
use crate::page::PageMessage;
use crate::page::PageSelector;
use crate::screenshot_tool::app_harness::AppHarness;

fn fake_book(id: &str, title: &str, authors: &[&str]) -> OnlineBook {
    OnlineBook {
        id: id.to_string(),
        title: title.to_string(),
        subtitle: None,
        authors: authors.iter().map(|a| a.to_string()).collect(),
        contributors: Vec::new(),
        summary: None,
        summary_html: None,
        language: Some("en".to_string()),
        publisher: None,
        identifier: None,
        published: None,
        rights: None,
        subject: None,
        // Deliberately `None`: a `Some(url)` here would trigger a real
        // `fetch_cover_bytes` HTTP call from `SearchCompleted`'s handler.
        cover_url: None,
        formats: vec![DownloadFormat {
            mime_type: "application/epub+zip".to_string(),
            href: format!("https://example.invalid/{id}.epub"),
            label: "EPUB".to_string(),
        }],
        catalog_name: "Project Gutenberg".to_string(),
    }
}

pub(in crate::screenshot_tool) async fn render(_sample_library: &Path) -> anyhow::Result<Vec<u8>> {
    let mut harness = AppHarness::new().await;
    harness
        .send(Message::ActivatePage(PageSelector::OnlineLibrary))
        .await;

    let books = vec![
        fake_book(
            "pg-84",
            "Frankenstein; Or, The Modern Prometheus",
            &["Mary Wollstonecraft Shelley"],
        ),
        fake_book("pg-1342", "Pride and Prejudice", &["Jane Austen"]),
        fake_book("pg-2701", "Moby-Dick; Or, The Whale", &["Herman Melville"]),
        fake_book(
            "pg-11",
            "Alice's Adventures in Wonderland",
            &["Lewis Carroll"],
        ),
    ];
    harness
        .send(Message::Page(Box::new(PageMessage::OnlineLibrary(
            OnlineLibraryMessage::SearchCompleted(books, std::collections::HashMap::new()),
        ))))
        .await;

    let mut renderer = HeadlessRenderer::with_theme(Theme::dark());
    Ok(harness.render_rgba(&mut renderer))
}
