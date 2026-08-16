//! Shared scaffolding for unit tests that need a real `ApplicationModule`/
//! `DocumentProvider` (backed by a temp-dir SQLite DB) without going
//! through the full BDD harness in `crate::bdd`.
#![cfg(any(test, feature = "screenshot-tool"))]

use std::path::PathBuf;
use std::sync::Arc;

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
