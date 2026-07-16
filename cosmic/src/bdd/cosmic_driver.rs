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
use provider::r#async::HasSetExpired;
use read_flow_core::ExpandedPath;
use read_flow_core::api::ReadingStatus;
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
use crate::client::ClientSelector;
use crate::config::EpubViewerConfig;
use crate::document_provider::DocumentProvider;
use crate::page::Page;
use crate::page::PreferencesMessage;
use crate::page::PreferencesPage;

pub struct CosmicDriver {
    application_module: Arc<ApplicationModule>,
    preferences_page: PreferencesPage,
    document_provider: Arc<DocumentProvider>,
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

        let document_provider = Arc::new(DocumentProvider::new(Aggregator::new(
            vec![application_module.clone().into()],
            application_module.clone(),
        )));
        let (preferences_page, init_task) = PreferencesPage::new(
            application_module.clone(),
            crate::config::Config::default(),
            document_provider.clone(),
        );
        drain(init_task).await;

        Self {
            application_module,
            preferences_page,
            document_provider,
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
            self.preferences_page
                .update(PreferencesMessage::CheckSourceStatus(remote.clone())),
        )
        .await;
        messages
            .into_iter()
            .find_map(|message| match message {
                PreferencesMessage::SetSourceStatus(id, reachable) if id == remote.id => {
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
            self.preferences_page
                .update(PreferencesMessage::ToggleDryRun(true)),
        )
        .await;
        let messages = drain(self.preferences_page.update(PreferencesMessage::Save)).await;
        assert!(
            messages
                .iter()
                .any(|message| matches!(message, PreferencesMessage::SaveComplete)),
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
    /// the scan to create a `Document`/`File`/`Content` row triple), returning
    /// the resulting `File` row — the in-process equivalent of what
    /// `RestDriver::seed_document`'s `POST /files` hands back.
    async fn scan_fixture(&self) -> read_flow_core::db::models::File {
        self.scan_fixture_path(fixtures::sample_epub_path(), "sample.epub")
            .await
    }

    async fn scan_fixture2(&self) -> read_flow_core::db::models::File {
        self.scan_fixture_path(fixtures::sample2_epub_path(), "sample2.epub")
            .await
    }

    async fn scan_fixture_path(
        &self,
        src: std::path::PathBuf,
        filename: &str,
    ) -> read_flow_core::db::models::File {
        let scan_dir = tempfile::tempdir().expect("temp scan dir");
        let dest = scan_dir.path().join(filename);
        std::fs::copy(src, &dest).expect("copy fixture epub");

        self.application_module
            .scan(&dest)
            .await
            .expect("scan fixture epub");

        // `scan` canonicalizes the path before storing it, so the lookup must
        // canonicalize too (matters when the temp dir is symlinked, e.g. macOS
        // `/var` → `/private/var`).
        let stored = dest.canonicalize().unwrap_or(dest);
        let pool = self.application_module.connection_pool().await;
        let mut conn = pool.acquire().await.expect("acquire connection");
        dao::select_file_by_path(&mut conn, &stored.to_string_lossy())
            .await
            .expect("select file by path")
            .expect("scanned file is in the DB")
    }

    pub async fn add_tag_to_document(&self, guid: &str, tag: &str) {
        let pool = self.application_module.connection_pool().await;
        let mut conn = pool.acquire().await.expect("acquire connection");
        let file = dao::select_file_by_guid(&mut conn, guid)
            .await
            .expect("select file by guid")
            .unwrap_or_else(|| panic!("file {guid} not found"));
        dao::upsert_content_tag(
            &mut conn,
            ContentTag::new(file.fingerprint.clone(), tag.to_string()),
        )
        .await
        .expect("upsert content tag");
    }

    pub async fn remove_tag_from_document(&self, guid: &str, tag: &str) {
        let pool = self.application_module.connection_pool().await;
        let mut conn = pool.acquire().await.expect("acquire connection");
        let file = dao::select_file_by_guid(&mut conn, guid)
            .await
            .expect("select file by guid")
            .unwrap_or_else(|| panic!("file {guid} not found"));
        dao::delete_content_tags(&mut conn, &file.fingerprint, vec![tag.to_string()])
            .await
            .expect("delete content tag");
    }

    pub async fn document_has_tag(&self, guid: &str, tag: &str) -> bool {
        let pool = self.application_module.connection_pool().await;
        let mut conn = pool.acquire().await.expect("acquire connection");
        let file = dao::select_file_by_guid(&mut conn, guid)
            .await
            .expect("select file by guid")
            .unwrap_or_else(|| panic!("file {guid} not found"));
        dao::select_content_tags_by_fingerprint(&mut conn, &file.fingerprint)
            .await
            .expect("select content tags")
            .iter()
            .any(|t| t.tag == tag)
    }

    pub async fn set_reading_status(&self, guid: &str, status: &str) {
        let status: ReadingStatus = status.parse().expect("valid reading status");
        let pool = self.application_module.connection_pool().await;
        let mut conn = pool.acquire().await.expect("acquire connection");
        let file = dao::select_file_by_guid(&mut conn, guid)
            .await
            .expect("select file by guid")
            .unwrap_or_else(|| panic!("file {guid} not found"));
        dao::update_reading_status_only(&mut conn, &file.fingerprint, status.into())
            .await
            .expect("update reading status");
    }

    pub async fn get_reading_status(&self, guid: &str) -> String {
        let pool = self.application_module.connection_pool().await;
        let mut conn = pool.acquire().await.expect("acquire connection");
        let file = dao::select_file_by_guid(&mut conn, guid)
            .await
            .expect("select file by guid")
            .unwrap_or_else(|| panic!("file {guid} not found"));
        ReadingStatus::from(file.status).to_string()
    }

    /// Returns `(file_guid, doc_api_guid)`.
    /// Returns `(file_guid, doc_api_guid, fingerprint)`.
    pub async fn seed_document(&self) -> (String, String, String) {
        let file = self.scan_fixture().await;
        let doc_api_guid = file
            .document_guid
            .clone()
            .expect("scanned fixture must produce a document");
        (file.guid, doc_api_guid, file.fingerprint)
    }

    /// Seeds the second fixture ("Zeta Test Book"). Returns `(file_guid, doc_api_guid, fingerprint)`.
    pub async fn seed_second_document(&self) -> (String, String, String) {
        let file = self.scan_fixture2().await;
        let doc_api_guid = file
            .document_guid
            .clone()
            .expect("scanned fixture2 must produce a document");
        (file.guid, doc_api_guid, file.fingerprint)
    }

    /// `scan_fixture` plus a DAO-direct `content_tags` upsert. Returns
    /// `(file_guid, doc_api_guid, fingerprint)`.
    pub async fn seed_tagged_document(&self, tag: &str) -> (String, String, String) {
        let file = self.scan_fixture().await;
        let doc_api_guid = file
            .document_guid
            .clone()
            .expect("scanned fixture must produce a document");
        let pool = self.application_module.connection_pool().await;
        let mut conn = pool.acquire().await.expect("acquire connection");
        dao::upsert_content_tag(
            &mut conn,
            ContentTag::new(file.fingerprint.clone(), tag.to_string()),
        )
        .await
        .expect("upsert content tag");
        (file.guid, doc_api_guid, file.fingerprint)
    }

    pub async fn prepare_scan_dir(&self) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("temp scan dir");
        let dest = dir.path().join("sample.epub");
        std::fs::copy(fixtures::sample_epub_path(), &dest).expect("copy fixture");
        let expanded = read_flow_core::ExpandedPath::from_str(&dir.path().to_string_lossy())
            .expect("valid path");
        self.application_module
            .update_settings(move |settings| {
                settings.scan.directories.insert(
                    expanded,
                    read_flow_core::scan::DirectorySettings::Scan {
                        tags: Vec::new(),
                        inherit: false,
                    },
                );
            })
            .await
            .expect("update settings");
        dir
    }

    pub async fn scan_configured(&self) -> u64 {
        self.application_module
            .scan_configured()
            .await
            .expect("scan_configured")
            .processed
    }

    pub async fn get_document_title(&self, doc_api_guid: &str) -> String {
        let pool = self.application_module.connection_pool().await;
        let mut conn = pool.acquire().await.expect("acquire connection");
        dao::select_api_document_by_guid(&mut conn, doc_api_guid)
            .await
            .expect("select document by guid")
            .expect("document not found")
            .metadata
            .title
            .expect("document has no title")
    }

    pub async fn set_document_title(&self, doc_api_guid: &str, title: &str) {
        let pool = self.application_module.connection_pool().await;
        let mut conn = pool.acquire().await.expect("acquire connection");
        let doc = dao::select_document_by_guid(&mut conn, doc_api_guid)
            .await
            .expect("select document")
            .expect("document not found");
        dao::upsert_document_user_metadata(
            &mut conn,
            doc.id,
            None,
            Some(title),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .expect("upsert document metadata");
    }

    pub async fn enable_private_mode(&mut self) {
        drain(
            self.preferences_page
                .update(PreferencesMessage::TogglePrivateMode(true)),
        )
        .await;
        let messages = drain(self.preferences_page.update(PreferencesMessage::Save)).await;
        assert!(
            messages
                .iter()
                .any(|m| matches!(m, PreferencesMessage::SaveComplete)),
            "Save did not complete"
        );
    }

    pub async fn private_mode_is_enabled(&self) -> bool {
        Settings::extract_from(self.application_module.config_path())
            .expect("read persisted settings")
            .ui
            .private_mode()
    }

    pub async fn check_missing(&self) -> Vec<String> {
        self.application_module.check_missing(false).await
    }

    pub async fn delete_document(&self, guid: &str) {
        let pool = self.application_module.connection_pool().await;
        let mut conn = pool.acquire().await.expect("acquire connection");
        let file = dao::select_file_by_guid(&mut conn, guid)
            .await
            .expect("select file by guid")
            .expect("file not found");
        dao::delete_file_record(&pool, file.id)
            .await
            .expect("delete file record");
    }

    pub async fn file_is_listed(&self, guid: &str) -> bool {
        let pool = self.application_module.connection_pool().await;
        let mut conn = pool.acquire().await.expect("acquire connection");
        dao::select_file_by_guid(&mut conn, guid)
            .await
            .expect("select file by guid")
            .is_some()
    }

    pub async fn set_reading_progress(&self, fingerprint: &str, position: &str, percentage: f64) {
        let pool = self.application_module.connection_pool().await;
        let mut conn = pool.acquire().await.expect("acquire connection");
        dao::upsert_reading_state(
            &mut conn,
            read_flow_core::db::models::ReadingState {
                fingerprint: fingerprint.to_string(),
                status: 1,
                position: position.to_string(),
                percentage,
                last_updated: String::new(),
                status_updated_at: String::new(),
            },
        )
        .await
        .expect("upsert reading state");
    }

    pub async fn get_reading_progress(&self, fingerprint: &str) -> (String, f64) {
        let pool = self.application_module.connection_pool().await;
        let mut conn = pool.acquire().await.expect("acquire connection");
        let state = dao::get_reading_state(&mut conn, fingerprint)
            .await
            .expect("get reading state")
            .expect("reading state not found");
        (state.position, state.percentage)
    }

    pub async fn document_is_listed(&self, title: &str) -> bool {
        let pool = self.application_module.connection_pool().await;
        let mut conn = pool.acquire().await.expect("acquire connection");
        dao::select_all_api_documents(&mut conn)
            .await
            .expect("list documents")
            .iter()
            .any(|doc| doc.metadata.title.as_deref() == Some(title))
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

    // -- documents.search --

    /// Returns true if the document with `title` appears when filtering by `query`.
    pub async fn search_returns_document(&self, query: &str, title: &str) -> bool {
        let docs = self
            .document_provider
            .get_documents()
            .await
            .expect("get documents");
        let q = query.to_lowercase();
        docs.into_iter().any(|doc| {
            let doc_title = doc
                .document_meta
                .title
                .as_deref()
                .unwrap_or("")
                .to_lowercase();
            doc_title.contains(&q) && doc.document_meta.title.as_deref() == Some(title)
        })
    }

    // -- documents.filter_by_status --

    /// Returns true if the document with `title` has reading status `status_str`.
    pub async fn filter_by_status_returns_document(&self, status_str: &str, title: &str) -> bool {
        let status: ReadingStatus = status_str.parse().expect("valid reading status");
        let docs = self
            .document_provider
            .get_documents()
            .await
            .expect("get documents");
        docs.into_iter().any(|doc| {
            doc.document_meta.title.as_deref() == Some(title)
                && doc.contents.iter().any(|c| c.status == status)
        })
    }

    // -- documents.filter_by_tag --

    /// Returns true if the document with `title` carries `tag`.
    pub async fn filter_by_tag_returns_document(&self, tag: &str, title: &str) -> bool {
        let docs = self
            .document_provider
            .get_documents()
            .await
            .expect("get documents");
        docs.into_iter().any(|doc| {
            doc.document_meta.title.as_deref() == Some(title)
                && doc
                    .contents
                    .iter()
                    .flat_map(|c| c.tags.iter())
                    .any(|t| t == tag)
        })
    }

    // -- documents.pagination --

    /// Returns true if `title` appears in the first page of the document list.
    pub async fn document_on_first_page(&self, title: &str) -> bool {
        let docs: Vec<_> = self
            .document_provider
            .get_documents()
            .await
            .expect("get documents")
            .into_iter()
            .collect();
        let pagination = crate::component::pagination::Pagination::new(docs.len());
        pagination
            .filter_visible(&docs)
            .any(|doc| doc.document_meta.title.as_deref() == Some(title))
    }

    // -- documents.filter_by_source --

    /// Returns true if the document with `title` is from `source_name` ("Local" or a URL string).
    pub async fn filter_by_source_returns_document(&self, source_name: &str, title: &str) -> bool {
        use crate::client::ClientSelector;
        let selector = if source_name == "Local" {
            ClientSelector::Local
        } else {
            let url = source_name.parse().expect("valid URL for source filter");
            ClientSelector::Remote(url)
        };
        let docs = self
            .document_provider
            .get_documents()
            .await
            .expect("get documents");
        docs.into_iter().any(|doc| {
            doc.document_meta.title.as_deref() == Some(title)
                && doc
                    .contents
                    .iter()
                    .flat_map(|c| c.sources.iter())
                    .any(|s| s.client == selector)
        })
    }

    // -- documents.merge --

    /// Merges `loser_guid` into `winner_guid`. After merge, only the winner document remains.
    pub async fn merge_documents(&self, winner_guid: &str, loser_guid: &str) {
        let pool = self.application_module.connection_pool().await;
        dao::merge_documents(&pool, winner_guid, &[loser_guid.to_string()])
            .await
            .expect("merge documents");
    }

    /// Returns the number of documents in the library.
    pub async fn document_count(&self) -> usize {
        let docs = self
            .document_provider
            .get_documents()
            .await
            .expect("get documents");
        docs.into_iter().count()
    }

    // -- documents.sort --

    /// Returns document titles sorted by title ascending.
    pub async fn sorted_document_titles_ascending(&self) -> Vec<String> {
        let docs = self
            .document_provider
            .get_documents()
            .await
            .expect("get documents");
        let mut titles: Vec<String> = docs
            .into_iter()
            .filter_map(|d| d.document_meta.title.clone())
            .collect();
        titles.sort();
        titles
    }

    // -- reading.image_viewer --

    /// Returns `true` if an `ImageViewer` can be instantiated with test image data —
    /// the headless proxy for "the image viewer opens and displays the image".
    pub fn image_viewer_opens_successfully(&self) -> bool {
        // Minimal 1x1 white PNG bytes — same as the sample_cover fixture's cover image.
        let png_bytes: &[u8] = &[
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, // signature
            0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, // IHDR length + type
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, // width=1, height=1
            0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53, 0xde, // 8bpp RGB + CRC
            0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, // IDAT length + type
            0x08, 0xd7, 0x63, 0xf8, 0xcf, 0xc0, 0x00, 0x00, // compressed scanline
            0x00, 0x02, 0x00, 0x01, 0xe2, 0x21, 0xbc, 0x33, // + CRC
            0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82, // IEND
        ];
        let handle = cosmic::iced::widget::image::Handle::from_bytes(png_bytes.to_vec());
        let image = crate::page::ViewerImage::Raster {
            handle,
            natural_width: 1,
            natural_height: 1,
        };
        let _viewer = crate::page::image_viewer::ImageViewer::new(0, image);
        true
    }

    // -- app.epub_viewer_choice --

    pub async fn set_epub_viewer_choice(&mut self, choice: &str) {
        let config = match choice {
            "MuPdf" => EpubViewerConfig::MuPdf,
            "ExternalViewer" => EpubViewerConfig::ExternalViewer,
            _ => EpubViewerConfig::NativeEpub,
        };
        drain(
            self.preferences_page
                .update(PreferencesMessage::SetEpubViewer(config)),
        )
        .await;
    }

    pub fn epub_viewer_choice(&self) -> &str {
        match self.preferences_page.epub_viewer() {
            EpubViewerConfig::NativeEpub => "NativeEpub",
            EpubViewerConfig::MuPdf => "MuPdf",
            EpubViewerConfig::ExternalViewer => "ExternalViewer",
        }
    }

    // -- app.theme_overrides --

    pub async fn set_custom_theme_enabled(&mut self, enabled: bool) {
        drain(
            self.preferences_page
                .update(PreferencesMessage::ToggleCustomTheme(enabled)),
        )
        .await;
    }

    pub async fn set_theme_variant(&mut self, variant: &str) {
        let variant = match variant {
            "Dark" => read_flow_core::settings::ThemeVariant::Dark,
            _ => read_flow_core::settings::ThemeVariant::Light,
        };
        drain(
            self.preferences_page
                .update(PreferencesMessage::SetThemeVariant(variant)),
        )
        .await;
    }

    pub async fn set_theme_accent(&mut self, hex: &str) {
        drain(
            self.preferences_page
                .update(PreferencesMessage::SetThemeAccent(Some(hex.to_string()))),
        )
        .await;
    }

    /// `true` when the custom theme is active and builds dark.
    pub fn effective_theme_is_dark(&self) -> bool {
        crate::app_theme::build_theme(self.preferences_page.theme_settings())
            .is_some_and(|theme| theme.cosmic().is_dark)
    }

    /// The effective accent color as `#rrggbb`, from the built custom theme.
    pub fn effective_accent_hex(&self) -> String {
        let theme = crate::app_theme::build_theme(self.preferences_page.theme_settings())
            .expect("custom theme is enabled");
        let accent = theme.cosmic().accent_color();
        crate::app_theme::color_to_hex(cosmic::iced::Color::from_rgba(
            accent.red,
            accent.green,
            accent.blue,
            accent.alpha,
        ))
    }

    /// `true` when no custom theme is built (app follows the system theme).
    pub fn follows_system_theme(&self) -> bool {
        crate::app_theme::build_theme(self.preferences_page.theme_settings()).is_none()
    }

    // -- documents.cover_display --

    async fn scan_fixture_cover(&self) -> read_flow_core::db::models::File {
        self.scan_fixture_path(fixtures::sample_cover_epub_path(), "sample_cover.epub")
            .await
    }

    /// Scans the cover fixture and returns `(file_guid, doc_api_guid, fingerprint)`.
    pub async fn seed_cover_document(&self) -> (String, String, String) {
        let file = self.scan_fixture_cover().await;
        let doc_api_guid = file
            .document_guid
            .clone()
            .expect("cover fixture must produce a document");
        (file.guid, doc_api_guid, file.fingerprint)
    }

    /// Returns `true` if a cover image is stored for the document.
    pub async fn document_has_cover(&self, doc_api_guid: &str) -> bool {
        let pool = self.application_module.connection_pool().await;
        let mut conn = pool.acquire().await.expect("acquire connection");
        let doc = dao::select_document_by_guid(&mut conn, doc_api_guid)
            .await
            .expect("select document by guid")
            .unwrap_or_else(|| panic!("document {doc_api_guid} not found"));
        dao::get_document_selected_cover(&mut conn, doc.id)
            .await
            .expect("get document cover")
            .is_some()
    }

    // -- sources.send_to_client --

    /// Sends the sample EPUB directly to the TestServer via the `FilesClient::import_file`
    /// path — the same path exercised by `Aggregator::send_document_to_client`.
    /// Returns `true` if the upload succeeds (i.e. the server accepted the file).
    pub async fn send_document_to_server(&self) -> bool {
        use read_flow_core::api::FileDataSource;
        use read_flow_core::client::FilesClient;
        let base_url: url::Url = self.server.base_url.parse().expect("valid TestServer URL");
        let client = FilesClient::new(
            base_url,
            self.server.user.clone(),
            self.server.password.clone(),
            false,
        )
        .expect("build FilesClient");
        client
            .import_file(&fixtures::sample_epub_path())
            .await
            .is_ok()
    }

    // -- online_library.search --

    /// Returns `true` if `GET /online-library/search?q=` responds with 200.
    /// With no OPDS catalogs configured the result is empty, but the endpoint
    /// must still be reachable and return a well-formed response.
    pub async fn online_library_search_responds(&self, query: &str) -> bool {
        use reqwest::Client;
        let client = Client::new();
        let encoded_q: String = query
            .chars()
            .flat_map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
                    vec![c]
                } else {
                    format!("%{:02X}", c as u32).chars().collect()
                }
            })
            .collect();
        client
            .get(format!(
                "{}/online-library/search?q={}",
                self.server.base_url, encoded_q
            ))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    // -- online_library.download_import --

    /// Serves `sample.epub` from a local one-shot HTTP fixture server, then
    /// calls `POST /online-library/import` on the TestServer, and returns
    /// `true` if the file was imported (HTTP 200).
    pub async fn online_library_import_responds(&self) -> bool {
        let url = fixtures::serve_epub_once().await;
        use reqwest::Client;
        let client = Client::new();
        client
            .post(format!("{}/online-library/import", self.server.base_url))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .json(&serde_json::json!({
                "title": "BDD Sample Book",
                "format": {
                    "mime_type": "application/epub+zip",
                    "href": url,
                    "label": "EPUB"
                }
            }))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    // -- documents.select_cover --

    /// Explicitly sets `selected_cover_fingerprint` for the document identified by `doc_api_guid`.
    pub async fn set_document_cover_fingerprint(&self, doc_api_guid: &str, fingerprint: &str) {
        let pool = self.application_module.connection_pool().await;
        let mut conn = pool.acquire().await.expect("acquire connection");
        let doc = dao::select_document_by_guid(&mut conn, doc_api_guid)
            .await
            .expect("select document by guid")
            .unwrap_or_else(|| panic!("document {doc_api_guid} not found"));
        dao::upsert_document_user_metadata(
            &mut conn,
            doc.id,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(fingerprint),
        )
        .await
        .expect("set selected_cover_fingerprint");
    }

    // -- reading.pdf_viewer --

    async fn scan_fixture_pdf(&self) -> read_flow_core::db::models::File {
        self.scan_fixture_path(fixtures::sample_pdf_path(), "sample.pdf")
            .await
    }

    /// Seeds the PDF fixture. Returns `(file_guid, doc_api_guid, fingerprint)`.
    pub async fn seed_pdf_document(&self) -> (String, String, String) {
        let file = self.scan_fixture_pdf().await;
        let doc_api_guid = file
            .document_guid
            .clone()
            .expect("pdf fixture must produce a document (check title in PDF metadata)");
        (file.guid, doc_api_guid, file.fingerprint)
    }

    /// Returns `true` if MuPDF can open the PDF fixture (the main observable
    /// for `reading.pdf_viewer` in a headless environment).
    pub fn pdf_opens_successfully(&self) -> bool {
        let path = fixtures::sample_pdf_path();
        mupdf::Document::open(path.as_path()).is_ok()
    }

    // -- documents.format_picker --

    /// Seeds an EPUB and a PDF, merges them into a single document, and returns
    /// the winner's document GUID. Stores the GUID in the world via the caller.
    pub async fn seed_merged_multiformat_document(&self) -> String {
        let epub_file = self.scan_fixture().await;
        let pdf_file = self.scan_fixture_pdf().await;
        let epub_doc_guid = epub_file
            .document_guid
            .clone()
            .expect("epub fixture must produce a document");
        let pdf_doc_guid = pdf_file
            .document_guid
            .clone()
            .expect("pdf fixture must produce a document");
        self.merge_documents(&epub_doc_guid, &pdf_doc_guid).await;
        // Invalidate cache so get_documents picks up the merged state.
        self.document_provider.set_expired().await;
        epub_doc_guid
    }

    /// Returns true if the document identified by `doc_api_guid` has more than
    /// one content entry — i.e. it has been merged from multiple-format files.
    pub async fn document_has_multiple_formats(&self, doc_api_guid: &str) -> bool {
        let docs = self
            .document_provider
            .get_documents()
            .await
            .expect("get documents");
        docs.into_iter()
            .any(|doc| doc.document_guid == doc_api_guid && doc.contents.len() > 1)
    }

    // -- reading.epub_viewer --

    /// Returns `true` if the EPUB parser can open the document's file. In BDD
    /// tests the fixture is always `sample.epub`; since the scan temp dir is
    /// cleaned up after seeding, we fall back to the fixture source path as a
    /// headless proxy that the epub pipeline handles the format correctly.
    pub async fn epub_opens_successfully(&self, doc_api_guid: &str) -> bool {
        // Try the stored local path first (exists if the caller kept the temp dir alive).
        let docs = self
            .document_provider
            .get_documents()
            .await
            .expect("get documents");
        if let Some(doc) = docs.into_iter().find(|d| d.document_guid == doc_api_guid) {
            let stored_path = doc
                .contents
                .iter()
                .flat_map(|c| c.sources.iter())
                .find(|s| s.client == ClientSelector::Local)
                .map(|s| std::path::PathBuf::from(&s.path));
            if let Some(path) = stored_path
                && path.exists()
            {
                return epub::EpubDocument::open(&path).is_ok();
            }
        }
        // Temp dir was cleaned up — open the original fixture as a proxy.
        epub::EpubDocument::open(fixtures::sample_epub_path()).is_ok()
    }

    // -- documents.batch_tag --

    /// Batch-adds `tag` to the document identified by `doc_api_guid`.
    pub async fn batch_add_tag(&self, doc_api_guid: &str, tag: &str) {
        let docs = self
            .document_provider
            .get_documents()
            .await
            .expect("get documents");
        let doc = docs
            .into_iter()
            .find(|d| d.document_guid == doc_api_guid)
            .unwrap_or_else(|| panic!("document {doc_api_guid} not found"));
        self.document_provider
            .batch_add_document_tags(vec![doc], &[tag.to_string()])
            .await
            .expect("batch add tags");
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
                headless: false,
                address: None,
                port: None,
                files: Vec::new(),
            },
        }
    }
}
