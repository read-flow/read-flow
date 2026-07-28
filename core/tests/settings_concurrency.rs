//! Regression tests for concurrent settings writes.
//!
//! `ApplicationModule::update_settings` is the single mutation path for the
//! configuration file (REST admin endpoints, GUI preferences via
//! `save_settings`). Before the write lock, two concurrent read-mutate-write
//! cycles could interleave and silently drop each other's changes (lost
//! update). These tests drive many concurrent updates and assert none are
//! lost.

use std::sync::Arc;

use read_flow_core::ApplicationModule;

#[tokio::test]
async fn concurrent_updates_are_not_lost() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("read-flow.toml");
    std::fs::write(
        &cfg,
        format!(
            "[database]\nurl = \"{}\"\n",
            dir.path().join("test.db").display()
        ),
    )
    .unwrap();

    let module = Arc::new(
        ApplicationModule::instantiate(cfg.clone())
            .await
            .expect("instantiate module"),
    );

    // 20 concurrent updates, each adding a distinct private tag. Without
    // write serialization most of these would overwrite each other.
    let mut handles = Vec::new();
    for i in 0..20 {
        let module = Arc::clone(&module);
        handles.push(tokio::spawn(async move {
            module
                .update_settings(move |settings| {
                    let mut tags = settings.ui.private_tags().to_vec();
                    tags.push(format!("tag-{i}"));
                    settings.ui.set_private_tags(tags);
                })
                .await
                .expect("update settings");
        }));
    }
    for handle in handles {
        handle.await.expect("join");
    }

    let settings = module.settings().await;
    let tags = settings.ui.private_tags();
    for i in 0..20 {
        assert!(
            tags.contains(&format!("tag-{i}")),
            "tag-{i} was lost; surviving tags: {tags:?}"
        );
    }
}

#[tokio::test]
async fn update_settings_sees_changes_written_by_other_writers() {
    // A writer that mutates via `update_settings` must observe a change
    // that another process wrote directly to the file (fresh read inside
    // the lock), not clobber it with a stale cached snapshot.
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("read-flow.toml");
    std::fs::write(
        &cfg,
        format!(
            "[database]\nurl = \"{}\"\n",
            dir.path().join("test.db").display()
        ),
    )
    .unwrap();

    let module = ApplicationModule::instantiate(cfg.clone())
        .await
        .expect("instantiate module");

    // Populate the settings cache with the initial (tagless) state.
    let _ = module.settings().await;

    // Simulate an external writer (e.g. another process) adding a tag.
    let mut on_disk = std::fs::read_to_string(&cfg).unwrap();
    on_disk.push_str("\n[ui]\nprivate_tags = [\"external\"]\n");
    std::fs::write(&cfg, on_disk).unwrap();

    // Our update must preserve the external change.
    module
        .update_settings(|settings| {
            let mut tags = settings.ui.private_tags().to_vec();
            tags.push("internal".to_string());
            settings.ui.set_private_tags(tags);
        })
        .await
        .expect("update settings");

    let settings = module.settings().await;
    let tags = settings.ui.private_tags();
    assert!(
        tags.contains(&"external".to_string()),
        "external tag lost: {tags:?}"
    );
    assert!(
        tags.contains(&"internal".to_string()),
        "internal tag lost: {tags:?}"
    );
}
