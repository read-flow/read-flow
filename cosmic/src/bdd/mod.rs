//! cucumber-rs BDD harness (Stage 5.2 of the PWA⇄COSMIC parity plan).
//!
//! Canonical `.feature` specs live at the workspace-root `features/` dir,
//! shared with the PWA's Playwright harness (`pwa/e2e/`). Drivers translate
//! the same Gherkin steps onto each surface's natural shape: `RestDriver`
//! hits the REST API directly, `CosmicDriver` drives `Page::update()`
//! headlessly (see `cosmic_driver` for why that's possible without a live
//! `cosmic::Core`).
//!
//! Run with `cargo nextest run -p read-flow bdd`. Select the driver via the
//! `BDD_DRIVER=rest|cosmic` env var (defaults to `rest`) — a single run boots
//! one driver, so scenarios are filtered to those carrying a matching
//! `@rest`/`@cosmic` tag (checked across feature- and scenario-level tags).
//! This keeps the world's driver and the executed scenarios in lockstep
//! without relying on cucumber CLI args reaching through `cargo test`'s own
//! arg parsing.
#![cfg(test)]

mod cosmic_driver;
mod driver;
mod fixtures;
mod rest_driver;
mod steps;
mod world;

use cucumber::World as _;

#[tokio::test]
async fn bdd() {
    let driver_tag = driver::Driver::env_name();
    world::BddWorld::cucumber()
        // Skip clap-parsing `std::env::args()` — nextest invokes the test
        // binary with libtest args (`--exact`, `--nocapture`, ...) that
        // aren't valid `cucumber` CLI options.
        .with_default_cli()
        .filter_run_and_exit(features_dir(), move |feature, _rule, scenario| {
            // gherkin strips the `@` from parsed tags.
            feature
                .tags
                .iter()
                .chain(&scenario.tags)
                .any(|tag| tag == driver_tag)
        })
        .await;
}

fn features_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../features")
}
