//! Shared full-application harness for screenshot scenes.
//!
//! `ReadFlow::view()` (the `cosmic::Application` trait method) only renders
//! the active page's content — the nav sidebar, header bar, and context
//! drawer are composed separately by `ApplicationExt::view_main()`, a
//! default trait method that calls `nav_bar()`/`header_start()`/etc. on
//! `&self` directly. That means the full window can be rendered by calling
//! `view_main()` on a plain `ReadFlow` instance — no `cosmic::app::Cosmic`
//! wrapper needed (which is good, since one of its fields is private to
//! libcosmic and can't be constructed from this crate).

use std::sync::Arc;
use std::sync::OnceLock;

use cosmic::Action;
use cosmic::Application as _;
use cosmic::ApplicationExt as _;
use cosmic::Core;
use cosmic::Task;
use cosmic_golden::HeadlessRenderer;

use crate::ApplicationModule;
use crate::app::Message;
use crate::app::ReadFlow;
use crate::document_provider::DocumentProvider;
use crate::logging::LogBus;

/// `logging::init()` installs a *global* tracing subscriber and panics if
/// called more than once per process — fine when only one old-style scene
/// needed a `LogBus`, but every scene now builds its own `ReadFlow` via
/// `AppHarness::new()`. Cache it so the real init only happens once.
fn log_bus() -> LogBus {
    static LOG_BUS: OnceLock<LogBus> = OnceLock::new();
    LOG_BUS.get_or_init(crate::logging::init).clone()
}

pub(crate) struct AppHarness {
    read_flow: ReadFlow,
    pub(crate) application_module: Arc<ApplicationModule>,
    _db_dir: tempfile::TempDir,
}

impl AppHarness {
    pub(crate) async fn new() -> Self {
        // `test_support::document_provider()` also hands back a
        // `DocumentProvider` of its own, but we deliberately don't keep it:
        // `ReadFlow::init()` below builds its OWN internal `DocumentProvider`
        // from the same `application_module`, and each `DocumentProvider`
        // has its own cache. Scanning fixtures through a separate instance
        // would invalidate a cache nothing else reads from, leaving
        // ReadFlow's pages stuck showing the "no documents" state from their
        // very first (pre-scan) load. Fixtures must go through ReadFlow's
        // own `document_provider()` instead.
        let (application_module, _unused_document_provider, db_dir) =
            crate::test_support::document_provider().await;

        let (mut read_flow, init_task) = ReadFlow::init(
            Core::default(),
            (application_module.clone(), Vec::new(), log_bus()),
        );
        drain_and_replay(&mut read_flow, init_task).await;

        Self {
            read_flow,
            application_module,
            _db_dir: db_dir,
        }
    }

    pub(crate) fn document_provider(&self) -> &Arc<DocumentProvider> {
        &self.read_flow.document_provider
    }

    /// Sends a message and drains + replays every task it returns (two
    /// levels deep), matching the pattern already proven for individual
    /// pages: most messages here trigger a `task::future` that itself
    /// yields a follow-up message (e.g. a DB read completing) which must be
    /// fed back into `update()` before `view_main()` reflects it.
    pub(crate) async fn send(&mut self, message: Message) {
        let task = self.read_flow.update(message);
        drain_and_replay(&mut self.read_flow, task).await;
    }

    /// Sends a message but does NOT drain the task it returns — for
    /// messages whose returned task would trigger real I/O (e.g. a network
    /// health check) that a screenshot must never perform. Rust futures do
    /// nothing until polled, so the discarded task simply never runs.
    pub(crate) fn send_without_draining(&mut self, message: Message) {
        let _ = self.read_flow.update(message);
    }

    pub(crate) fn render_rgba(&self, renderer: &mut HeadlessRenderer) -> Vec<u8> {
        let element = self.read_flow.view_main();
        renderer.render(element, super::WIDTH, super::HEIGHT)
    }
}

async fn drain_and_replay(read_flow: &mut ReadFlow, task: Task<Action<Message>>) {
    for message in crate::test_support::drain(task).await {
        let follow_up = read_flow.update(message);
        for message in crate::test_support::drain(follow_up).await {
            let _ = read_flow.update(message);
        }
    }
}
