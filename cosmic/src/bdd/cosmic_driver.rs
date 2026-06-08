//! Drives Gherkin steps against COSMIC's own logic layer — headlessly,
//! without a live `cosmic::Core`/winit/theme. `Page::update()` returns
//! `Task<Action<Message>>`; `into_stream` exposes its underlying
//! `BoxStream<RuntimeAction<Action<Message>>>`, so polling it and unwrapping
//! `RuntimeAction::Output(Action::App(message))` drives a page exactly like
//! the live runtime would, minus rendering. Validated by a throwaway spike
//! (`SourcesPage::new`/`update(CheckSourceStatus)` ran cleanly and yielded
//! the expected `SetSourceStatus` — see git history for the spike if this
//! comment needs more context).
//!
//! True pixel-level GUI automation is out of scope (libcosmic/iced tooling
//! for that is immature) — this validates *behavior*, the same thing the
//! REST/Playwright drivers validate, just at COSMIC's logic boundary.

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use cosmic::Action;
use cosmic::Task;
use cosmic::iced::runtime::Action as RuntimeAction;
use cosmic::iced::runtime::task::into_stream;
use futures::StreamExt;
use read_flow_core::ExpandedPath;
use read_flow_core::db::dao;
use read_flow_core::db::models::ContentTag;
use read_flow_core::db::models::NewRemote;
use read_flow_core::db::models::Remote;
use read_flow_core::scan::DirectorySettings;
use read_flow_core::settings::HashedPassword;
use read_flow_core::settings::Settings;
use read_flow_core::settings::UserEntry;
use read_flow_core::test_support::TestServer;

use crate::AppSettings;
use crate::ApplicationModule;
use crate::Cli;
use crate::aggregator::Aggregator;
use crate::bdd::fixtures;
use crate::bdd::rest_driver;
use crate::document_provider::DocumentProvider;
use crate::page::Page;
use crate::page::SettingsMessage;
use crate::page::SettingsPage;
use crate::page::SourcesMessage;
use crate::page::SourcesPage;

pub struct CosmicDriver {
    application_module: Arc<ApplicationModule>,
    sources_page: SourcesPage,
    settings_page: SettingsPage,
    /// A real, network-reachable backend for `Remote`s to point at —
    /// `CheckSourceStatus` makes an actual HTTP call, so there must be
    /// something on the other end (mirrors what `RestDriver` boots, and what
    /// a real COSMIC instance would be checking against).
    server: TestServer,
    /// Kept alive for the lifetime of the driver — the temp DB lives here.
    _temp_dir: tempfile::TempDir,
    /// Set by `register_remote` (`remotes_manage`), consumed by
    /// `remove_registered_remote`.
    registered_remote: Option<Remote>,
}

impl CosmicDriver {
    pub async fn new() -> Self {
        let server = TestServer::spawn(rest_driver::USER, rest_driver::PASSWORD).await;
        let temp_dir = tempfile::tempdir().expect("temp dir for cosmic driver");
        let config_path = temp_dir.path().join("read-flow.toml");
        std::fs::write(
            &config_path,
            format!(
                "[database]\nurl = \"{}\"\n",
                temp_dir.path().join("test.db").display()
            ),
        )
        .expect("write temp config");

        let application_module = Arc::new(
            ApplicationModule::new(AppSettings::for_test(config_path.clone()), config_path)
                .await
                .expect("build application module"),
        );

        let (sources_page, init_task) = SourcesPage::new(application_module.clone());
        drain(init_task).await;

        // Mirrors `Pages::new`'s construction (see `page/mod.rs`) — `SettingsPage`
        // needs a `DocumentProvider` for its private-tags `TagEditor`, built the
        // same way: an `Aggregator` over just the local client.
        let document_provider = Arc::new(DocumentProvider::new(Aggregator::new(
            vec![application_module.clone().into()],
            application_module.clone(),
        )));
        let (settings_page, init_settings_task) =
            SettingsPage::new(application_module.clone(), document_provider);
        drain(init_settings_task).await;

        Self {
            application_module,
            sources_page,
            settings_page,
            server,
            _temp_dir: temp_dir,
            registered_remote: None,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.server.base_url
    }

    /// Inserts a `Remote` directly via the DAO — the natural COSMIC-side
    /// equivalent of "register a remote source" (no UI form to drive).
    pub async fn insert_remote(&self, base_url: &str, user_id: &str, passphrase: &str) -> Remote {
        let pool = self.application_module.connection_pool().await;
        let mut conn = pool.acquire().await.expect("acquire connection");
        dao::insert_remote(
            &mut conn,
            NewRemote {
                base_url: base_url.to_string(),
                order: 0,
                passphrase: passphrase.to_string(),
                user_id: user_id.to_string(),
            },
        )
        .await
        .expect("insert remote")
    }

    /// Drives `CheckSourceStatus` to completion and returns the reachability
    /// it reports via `SetSourceStatus` — the observable behavior, same as
    /// what the PWA asserts on screen and RestDriver reads from `/status`.
    pub async fn check_source_status(&mut self, remote: &Remote) -> bool {
        let messages = drain(
            self.sources_page
                .update(SourcesMessage::CheckSourceStatus(remote.clone())),
        )
        .await;
        messages
            .into_iter()
            .find_map(|message| match message {
                SourcesMessage::SetSourceStatus(id, reachable) if id == remote.id => {
                    Some(reachable)
                }
                _ => None,
            })
            .expect("CheckSourceStatus did not yield a matching SetSourceStatus")
    }

    /// `remotes_manage`'s "register a remote source" — same DAO-direct bypass
    /// as `insert_remote`, keeping the inserted row around for a later
    /// `remove_registered_remote`.
    pub async fn register_remote(&mut self, user_id: &str, passphrase: &str) {
        let base_url = self.base_url().to_string();
        let remote = self.insert_remote(&base_url, user_id, passphrase).await;
        self.registered_remote = Some(remote);
    }

    /// Removes the row `register_remote` inserted, directly via the DAO —
    /// driving the UI's delete-confirmation dialog headlessly would mean
    /// re-implementing iced's runtime loop for `RequestDeleteSource` →
    /// `ConfirmDeleteSource` → `DeleteSource` → `DeletedSource`'s chained
    /// `Task`s. The observable this scenario cares about (does the list
    /// reflect the removal?) is verified via `remote_count`, same as the DAO
    /// the real `DeleteSource` handler calls.
    pub async fn remove_registered_remote(&mut self) {
        let remote = self
            .registered_remote
            .take()
            .expect("no remote was registered to remove");
        let pool = self.application_module.connection_pool().await;
        dao::delete_remote_by_id(&pool, remote.id)
            .await
            .expect("delete remote");
    }

    pub async fn remote_count(&self) -> usize {
        let pool = self.application_module.connection_pool().await;
        let mut conn = pool.acquire().await.expect("acquire connection");
        dao::select_all_remotes(&mut conn)
            .await
            .expect("list remotes")
            .len()
    }

    /// Drives `ToggleDryRun` then `Save` to completion — the same two-message
    /// path the real Server settings UI takes (toggle the switch, press Save).
    /// `drain` only follows single-hop chains (see its doc comment), but
    /// `Save`'s `task::future` resolving to `SaveComplete` is exactly that.
    pub async fn enable_dry_run_and_save(&mut self) {
        drain(
            self.settings_page
                .update(SettingsMessage::ToggleDryRun(true)),
        )
        .await;
        let messages = drain(self.settings_page.update(SettingsMessage::Save)).await;
        assert!(
            messages
                .iter()
                .any(|message| matches!(message, SettingsMessage::SaveComplete)),
            "Save did not complete"
        );
    }

    /// Re-reads the persisted config from disk — the same observable the REST
    /// `GET /settings` and the PWA's re-fetch verify, just at COSMIC's own
    /// storage boundary (its `read-flow.toml`, not a remote's REST endpoint).
    pub async fn dry_run_is_enabled(&self) -> bool {
        Settings::extract_from(self.application_module.config_path())
            .expect("read persisted settings")
            .scan
            .dry_run
    }

    /// `AddDirectory`'s real path spawns a `DirectorySettingsForm` and only
    /// inserts on `SaveDirectory` once the editor confirms — driving that
    /// headlessly would mean reproducing the form's own multi-hop lifecycle
    /// for no extra coverage. Same DAO-direct bypass as `insert_remote`:
    /// `update_settings` is exactly what `SaveDirectory` + `Save` end up
    /// calling, just without the in-between editor state.
    pub async fn add_scan_directory(&self, path: &str) {
        let expanded = ExpandedPath::from_str(path).expect("valid path");
        self.application_module
            .update_settings(move |settings| {
                settings.scan.directories.insert(
                    expanded,
                    DirectorySettings::Scan {
                        tags: Vec::new(),
                        inherit: false,
                    },
                );
            })
            .await
            .expect("update settings");
    }

    pub async fn scan_directory_is_listed(&self, path: &str) -> bool {
        let expanded = ExpandedPath::from_str(path).expect("valid path");
        Settings::extract_from(self.application_module.config_path())
            .expect("read persisted settings")
            .scan
            .directories
            .contains_key(&expanded)
    }

    /// `AddAuthorizedUser`'s real path spawns an `AuthorizedUserForm` — same
    /// multi-hop-form bypass as `add_scan_directory`. `update_settings` is
    /// exactly what the form's `Submit` handler ends up calling.
    pub async fn add_user(&self, user_id: &str, password: &str) {
        let entry = UserEntry::Simple(
            HashedPassword::try_from(password.to_string()).expect("hash password"),
        );
        let id = user_id.to_string();
        self.application_module
            .update_settings(move |settings| {
                settings.server.authorized_users.insert(id, entry);
            })
            .await
            .expect("update settings");
    }

    pub async fn user_is_listed(&self, user_id: &str) -> bool {
        Settings::extract_from(self.application_module.config_path())
            .expect("read persisted settings")
            .server
            .authorized_users
            .contains_key(user_id)
    }

    /// Copies the shared `sample.epub` fixture into a fresh temp dir and
    /// scans it via `application_module.scan` (awaited to completion — see
    /// `tags_list.feature`'s doc comment for why a real EPUB is required for
    /// the scan to create a `Document`/`File`/`Content` row triple), then
    /// upserts a `content_tags` row directly — the in-process equivalent of
    /// `RestDriver::seed_tagged_document`'s `POST /files` + `POST .../tags`.
    pub async fn seed_tagged_document(&self, tag: &str) -> String {
        let scan_dir = tempfile::tempdir().expect("temp scan dir");
        let dest = scan_dir.path().join("sample.epub");
        std::fs::copy(fixtures::sample_epub_path(), &dest).expect("copy fixture epub");

        self.application_module
            .scan(&dest)
            .await
            .expect("scan fixture epub");

        let pool = self.application_module.connection_pool().await;
        let mut conn = pool.acquire().await.expect("acquire connection");
        let file = dao::select_file_by_path(&mut conn, &dest.to_string_lossy())
            .await
            .expect("select file by path")
            .expect("scanned file is in the DB");
        dao::upsert_content_tag(
            &mut conn,
            ContentTag::new(file.fingerprint.clone(), tag.to_string()),
        )
        .await
        .expect("upsert content tag");
        file.guid
    }

    pub async fn tag_is_listed(&self, tag: &str) -> bool {
        let pool = self.application_module.connection_pool().await;
        let mut conn = pool.acquire().await.expect("acquire connection");
        dao::select_all_distinct_tags(&mut conn)
            .await
            .expect("select distinct tags")
            .iter()
            .any(|t| t == tag)
    }
}

/// Polls a `Task` to completion, collecting the application messages it
/// yields. Other `RuntimeAction` variants (font loading, widget/clipboard/
/// window ops, ...) only matter to a live `cosmic::Core` and are skipped.
async fn drain<M: Send + 'static>(task: Task<Action<M>>) -> Vec<M> {
    let Some(mut stream) = into_stream(task) else {
        return Vec::new();
    };
    let mut messages = Vec::new();
    while let Some(action) = stream.next().await {
        if let RuntimeAction::Output(Action::App(message)) = action {
            messages.push(message);
        }
    }
    messages
}

impl AppSettings {
    fn for_test(config_path: PathBuf) -> Self {
        Self {
            cli_parameters: Cli {
                configuration_file: Some(config_path),
                private_mode: false,
                private_tags: Vec::new(),
                files: Vec::new(),
            },
        }
    }
}
