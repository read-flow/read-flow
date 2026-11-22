//! Shared scaffolding for unit tests that need a real `ApplicationModule`/
//! `DocumentProvider` (backed by a temp-dir SQLite DB) without going
//! through the full BDD harness in `crate::bdd`.
#![cfg(any(test, feature = "screenshot-tool"))]

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::MutexGuard;
use std::sync::OnceLock;

use cosmic::Action;
use cosmic::Task;
use cosmic::iced::runtime::Action as RuntimeAction;
use cosmic::iced::runtime::task::into_stream;
use futures::StreamExt as _;
use provider::r#async::HasSetExpired;
use read_flow_core::db::LOCAL_USER_ID;

use crate::AppSettings;
use crate::ApplicationModule;
use crate::Cli;
use crate::aggregator::Aggregator;
use crate::aggregator::Document;
use crate::document_provider::DocumentProvider;
use crate::logging::LogBus;

/// `logging::init()` installs a *global* tracing subscriber and panics if
/// called more than once per process. Unit tests across the crate run in
/// the same process, so share one `LogBus` here instead of each test
/// calling `logging::init()` directly.
pub(crate) fn log_bus() -> LogBus {
    static LOG_BUS: OnceLock<LogBus> = OnceLock::new();
    LOG_BUS.get_or_init(crate::logging::init).clone()
}

/// `apply_interface_font`/`apply_monospace_font` write the process-global
/// `cosmic::config::COSMIC_TK`. `#[golden_test]` snapshot renders read that
/// same global and assume it stays pinned to the bundled test fonts
/// (`cosmic_golden::init`) — under `cargo test`'s default threaded runner,
/// unit tests that mutate it can interleave with a concurrently running
/// golden-test render, corrupting the font it renders with and producing a
/// spurious snapshot mismatch. Both sides take this lock around their
/// `COSMIC_TK`-touching section so they never overlap.
pub(crate) fn cosmic_tk_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A real `ApplicationModule`/`DocumentProvider` pair backed by a fresh
/// temp-dir SQLite DB. Keep the returned `TempDir` alive for as long as the
/// DB is used.
pub(crate) async fn document_provider() -> (
    Arc<ApplicationModule>,
    Arc<DocumentProvider>,
    tempfile::TempDir,
) {
    let temp_dir = tempfile::tempdir().expect("temp dir");
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
        ApplicationModule::new(
            AppSettings {
                cli_parameters: Cli {
                    configuration_file: Some(config_path.clone()),
                    private_mode: false,
                    private_tags: Vec::new(),
                    headless: false,
                    address: None,
                    port: None,
                    debug: false,
                    files: Vec::new(),
                },
            },
            config_path,
        )
        .await
        .expect("build application module"),
    );
    let document_provider = Arc::new(DocumentProvider::new(Aggregator::new(
        vec![application_module.clone().into()],
        application_module.clone(),
    )));
    (application_module, document_provider, temp_dir)
}

/// Copies `src` into a fresh temp dir as `filename`, scans it, and fetches
/// back the resulting `Document`. Keep the returned `TempDir` alive for as
/// long as the viewer needs to read the file.
pub(crate) async fn scan_and_fetch_document(
    application_module: &ApplicationModule,
    document_provider: &DocumentProvider,
    src: PathBuf,
    filename: &str,
) -> (Document, tempfile::TempDir) {
    let scan_dir = tempfile::tempdir().expect("temp scan dir");
    let dest = scan_dir.path().join(filename);
    std::fs::copy(&src, &dest).expect("copy fixture");

    application_module.scan(&dest).await.expect("scan fixture");
    // The document provider caches `get_documents()` results until told
    // otherwise; invalidate so the just-scanned document is visible below.
    // (Callers that scan more than once per `document_provider` need this,
    // since nothing else drives cache invalidation off a plain `scan()`.)
    document_provider.set_expired().await;

    let stored = dest.canonicalize().unwrap_or(dest);
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.expect("acquire connection");
    let file = read_flow_core::db::dao::select_file_by_path(
        &mut conn,
        LOCAL_USER_ID,
        &stored.to_string_lossy(),
    )
    .await
    .expect("select file by path")
    .expect("scanned file is in the DB");
    drop(conn);

    let doc_api_guid = file
        .document_guid
        .clone()
        .expect("scanned file has a document");
    let document = document_provider
        .get_documents()
        .await
        .expect("get documents")
        .into_iter()
        .find(|d| d.document_guid == doc_api_guid)
        .expect("scanned document present");
    (document, scan_dir)
}

/// Polls a `Task` to completion, collecting the application messages it
/// yields. Mirrors `bdd::cosmic_driver::drain` (private to that module) —
/// see its doc comment for why other `RuntimeAction` variants are skipped.
pub(crate) async fn drain<M: Send + 'static>(task: Task<Action<M>>) -> Vec<M> {
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

/// Bounds found by laying out an element, see [`layout_extents`].
#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct LayoutExtents {
    /// Viewport and content bounds of every scrollable, in tree order.
    pub scrollables: Vec<ScrollableExtent>,
    /// Bounds of every container that has an id, in tree order.
    pub containers: Vec<(cosmic::widget::Id, cosmic::iced::Rectangle)>,
}

/// Viewport and content bounds of one scrollable in a laid-out element.
#[cfg(test)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct ScrollableExtent {
    pub viewport: cosmic::iced::Rectangle,
    pub content: cosmic::iced::Rectangle,
}

/// Lay out `element` at `width` × `height` with the real (headless) renderer
/// and return the bounds of its scrollables and of its containers with ids.
///
/// Lets tests check what the user would see — e.g. whether content overflows
/// its scrollable — without rendering pixels.
#[cfg(test)]
pub(crate) fn layout_extents<Message>(
    element: cosmic::Element<'_, Message>,
    width: f32,
    height: f32,
) -> LayoutExtents {
    use cosmic::iced::Rectangle;
    use cosmic::iced::Size;
    use cosmic::iced::Vector;
    use cosmic::iced::advanced::widget::Id;
    use cosmic::iced::advanced::widget::Operation;
    use cosmic::iced::advanced::widget::operation::Scrollable;
    use cosmic::iced::core::renderer::Headless;
    use cosmic::iced::runtime::user_interface::Cache;
    use cosmic::iced::runtime::user_interface::UserInterface;

    struct Collect(LayoutExtents);

    impl Operation<()> for Collect {
        fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<()>)) {
            operate(self);
        }

        fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
            if let Some(id) = id {
                self.0.containers.push((id.clone(), bounds));
            }
        }

        fn scrollable(
            &mut self,
            _id: Option<&Id>,
            bounds: Rectangle,
            content_bounds: Rectangle,
            _translation: Vector,
            _state: &mut dyn Scrollable,
        ) {
            self.0.scrollables.push(ScrollableExtent {
                viewport: bounds,
                content: content_bounds,
            });
        }
    }

    let mut renderer = futures::executor::block_on(<cosmic::Renderer as Headless>::new(
        cosmic::font::default(),
        cosmic::iced::Pixels(16.0),
        Some("tiny-skia"),
    ))
    .expect("create headless renderer");
    let mut ui = UserInterface::build(
        element,
        Size::new(width, height),
        Cache::default(),
        &mut renderer,
    );
    let mut collect = Collect(LayoutExtents::default());
    ui.operate(&renderer, &mut collect);
    collect.0
}
