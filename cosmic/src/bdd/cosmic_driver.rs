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
use std::sync::Arc;

use cosmic::Action;
use cosmic::Task;
use cosmic::iced::runtime::Action as RuntimeAction;
use cosmic::iced::runtime::task::into_stream;
use futures::StreamExt;
use read_flow_core::db::dao;
use read_flow_core::db::models::NewRemote;
use read_flow_core::db::models::Remote;
use read_flow_core::test_support::TestServer;

use crate::AppSettings;
use crate::ApplicationModule;
use crate::Cli;
use crate::bdd::rest_driver;
use crate::page::Page;
use crate::page::SourcesMessage;
use crate::page::SourcesPage;

pub struct CosmicDriver {
    application_module: Arc<ApplicationModule>,
    sources_page: SourcesPage,
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

        Self {
            application_module,
            sources_page,
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
