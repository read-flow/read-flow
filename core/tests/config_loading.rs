//! Regression test: the CLI/library `SettingsProvider` must load settings from
//! the configuration path it is given, not from the current working directory.
//!
//! Before the fix, `SettingsProvider` was a unit struct whose `provide()` called
//! `Settings::extract()` (which reads `./read-flow.toml`), so `--configuration-file`
//! was silently ignored.

use std::fs;

use provider::r#async::Provider;
use read_flow_core::SettingsProvider;

#[tokio::test]
async fn settings_provider_reads_the_given_config_path() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("custom-config.toml");
    fs::write(
        &cfg,
        "[database]\nurl = \"/tmp/distinctive-marker-path.db\"\n",
    )
    .unwrap();

    let settings = SettingsProvider {
        config_path: cfg.clone(),
    }
    .provide()
    .await
    .expect("load settings from the given path");

    assert!(
        format!("{:?}", settings.database.url()).contains("distinctive-marker-path"),
        "expected config loaded from {cfg:?}, got url {:?}",
        settings.database.url(),
    );
}
