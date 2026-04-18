// SPDX-License-Identifier: GPL-3.0-or-later
//
// Integration tests that make real network requests.
// Run with: cargo nextest run -p read-flow-core --features online_tests

#![cfg(feature = "online_tests")]

use read_flow_core::online_library::OnlineCatalog;
use read_flow_core::online_library::OnlineLibraryClient;
use read_flow_core::online_library::OpdsClient;

async fn assert_search_returns_results(catalog: OnlineCatalog, query: &str) {
    let client = OpdsClient::new(catalog.clone());
    let results = client
        .search(query)
        .await
        .unwrap_or_else(|e| panic!("search failed for '{}': {e}", catalog.name));
    assert!(
        !results.is_empty(),
        "expected at least one result from '{}' for query '{query}'",
        catalog.name
    );
    for book in &results {
        assert!(
            !book.title.is_empty(),
            "book from '{}' has empty title",
            catalog.name
        );
        assert!(
            !book.formats.is_empty(),
            "book '{}' from '{}' has no download formats",
            book.title,
            catalog.name
        );
    }
}

#[tokio::test]
async fn project_gutenberg_search_returns_results() {
    assert_search_returns_results(OnlineCatalog::project_gutenberg(), "moby dick").await;
}

#[tokio::test]
async fn standard_ebooks_search_returns_results() {
    assert_search_returns_results(OnlineCatalog::standard_ebooks(), "dickens").await;
}
