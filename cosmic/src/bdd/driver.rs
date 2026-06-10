//! Picks which surface a scenario run exercises. Step definitions dispatch
//! through this enum rather than hard-coding a driver, so the same Gherkin
//! steps run against either surface depending on `BDD_DRIVER`.

use crate::bdd::cosmic_driver::CosmicDriver;
use crate::bdd::rest_driver::RestDriver;

pub enum Driver {
    Rest(RestDriver),
    Cosmic(CosmicDriver),
}

impl Driver {
    /// Selects the driver via `BDD_DRIVER=rest|cosmic` (default `rest`).
    /// Must be paired with a matching cucumber tag filter — see `bdd::mod`.
    pub async fn new() -> Self {
        match env_name() {
            "cosmic" => Self::Cosmic(CosmicDriver::new().await),
            _ => Self::Rest(RestDriver::new().await),
        }
    }

    /// Same selection as [`Self::new`], without booting anything — used to
    /// derive the scenario tag filter (`@rest`/`@cosmic`) up front.
    pub fn env_name() -> &'static str {
        env_name()
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Rest(_) => "rest",
            Self::Cosmic(_) => "cosmic",
        }
    }

    /// The booted backend's URL — both drivers boot a real `TestServer`
    /// (REST hits it directly; COSMIC needs it as something for `Remote`s
    /// to actually reach over HTTP).
    pub fn base_url(&self) -> &str {
        match self {
            Self::Rest(driver) => driver.base_url(),
            Self::Cosmic(driver) => driver.base_url(),
        }
    }

    // -- _smoke_rest --

    pub async fn status_is_healthy(&self) -> bool {
        match self {
            Self::Rest(driver) => driver.status_is_healthy().await,
            Self::Cosmic(_) => panic!(
                "this step only supports the `rest` driver (run with BDD_DRIVER=rest and a matching @rest tag filter)"
            ),
        }
    }

    // -- remotes_status --

    /// "Add that server as a remote source" has no single natural shape
    /// across surfaces — REST has no "add remote" concept (the When step
    /// maps to "call /status with these creds" directly), while COSMIC
    /// inserts a `Remote` row pointing at the driver's own booted backend
    /// and drives `CheckSourceStatus`. Each driver returns the same
    /// observable: is the source reported as reachable?
    pub async fn add_remote_and_check_status(&mut self, user: &str, passphrase: &str) -> bool {
        match self {
            Self::Rest(driver) => driver.status_with(user, passphrase).await,
            Self::Cosmic(driver) => {
                let base_url = driver.base_url().to_string();
                let remote = driver.insert_remote(&base_url, user, passphrase).await;
                driver.check_source_status(&remote).await
            }
        }
    }

    // -- remotes_manage --
    // No REST surface — `remotes.manage` is client-side bookkeeping in both
    // apps (COSMIC's local DAO, the PWA's IndexedDB), so these only need to
    // support `pwa`/`cosmic` and a scenario carrying just those tags never
    // reaches the `Rest` branch (see `bdd::mod`'s tag-driven filter).

    pub async fn register_remote(&mut self, user: &str, passphrase: &str) {
        match self {
            Self::Rest(_) => panic!(
                "`remotes.manage` has no REST surface — run with BDD_DRIVER=pwa or BDD_DRIVER=cosmic"
            ),
            Self::Cosmic(driver) => driver.register_remote(user, passphrase).await,
        }
    }

    pub async fn remove_registered_remote(&mut self) {
        match self {
            Self::Rest(_) => panic!(
                "`remotes.manage` has no REST surface — run with BDD_DRIVER=pwa or BDD_DRIVER=cosmic"
            ),
            Self::Cosmic(driver) => driver.remove_registered_remote().await,
        }
    }

    pub async fn remote_count(&self) -> usize {
        match self {
            Self::Rest(_) => panic!(
                "`remotes.manage` has no REST surface — run with BDD_DRIVER=pwa or BDD_DRIVER=cosmic"
            ),
            Self::Cosmic(driver) => driver.remote_count().await,
        }
    }

    // -- admin_server_settings --
    // REST and COSMIC manage their own booted backend's settings directly —
    // "viewing" needs no navigation, so there's no dispatcher for that step
    // (see `steps/admin_server_settings.rs`). The PWA leg registers the
    // backend as a remote source first and lives entirely in `pwa/e2e/steps`.

    pub async fn enable_dry_run_and_save(&mut self) {
        match self {
            Self::Rest(driver) => driver.enable_dry_run_and_save().await,
            Self::Cosmic(driver) => driver.enable_dry_run_and_save().await,
        }
    }

    pub async fn dry_run_is_enabled(&self) -> bool {
        match self {
            Self::Rest(driver) => driver.dry_run_is_enabled().await,
            Self::Cosmic(driver) => driver.dry_run_is_enabled().await,
        }
    }

    // -- admin_scan_directories --
    // Same "REST/Cosmic manage their own config directly, no navigation"
    // mapping as `admin_server_settings` — see that section's comment and the
    // feature's doc comment. The PWA leg lives in `pwa/e2e/steps`.

    pub async fn add_scan_directory(&mut self, path: &str) {
        match self {
            Self::Rest(driver) => driver.add_scan_directory(path).await,
            Self::Cosmic(driver) => driver.add_scan_directory(path).await,
        }
    }

    pub async fn scan_directory_is_listed(&self, path: &str) -> bool {
        match self {
            Self::Rest(driver) => driver.scan_directory_is_listed(path).await,
            Self::Cosmic(driver) => driver.scan_directory_is_listed(path).await,
        }
    }

    // -- admin_authorized_users --
    // Same "REST/Cosmic manage their own config directly, no navigation"
    // mapping as `admin_scan_directories` — see that section's comment and the
    // feature's doc comment. The PWA leg lives in `pwa/e2e/steps`.

    pub async fn add_user(&mut self, user_id: &str, password: &str) {
        match self {
            Self::Rest(driver) => driver.add_user(user_id, password).await,
            Self::Cosmic(driver) => driver.add_user(user_id, password).await,
        }
    }

    pub async fn user_is_listed(&self, user_id: &str) -> bool {
        match self {
            Self::Rest(driver) => driver.user_is_listed(user_id).await,
            Self::Cosmic(driver) => driver.user_is_listed(user_id).await,
        }
    }

    // -- tags_list --
    // First scenario needing a seeded document — both drivers upload/scan the
    // shared `features/fixtures/sample.epub` and tag it; see each driver's
    // `seed_tagged_document` and the feature's doc comment.

    /// Returns `(file_guid, doc_api_guid, fingerprint)`.
    pub async fn seed_tagged_document(&self, tag: &str) -> (String, String, String) {
        match self {
            Self::Rest(driver) => driver.seed_tagged_document(tag).await,
            Self::Cosmic(driver) => driver.seed_tagged_document(tag).await,
        }
    }

    pub async fn tag_is_listed(&self, tag: &str) -> bool {
        match self {
            Self::Rest(driver) => driver.tag_is_listed(tag).await,
            Self::Cosmic(driver) => driver.tag_is_listed(tag).await,
        }
    }

    // -- tags_add --

    pub async fn add_tag_to_document(&self, guid: &str, tag: &str) {
        match self {
            Self::Rest(driver) => driver.add_tag_to_document(guid, tag).await,
            Self::Cosmic(driver) => driver.add_tag_to_document(guid, tag).await,
        }
    }

    pub async fn remove_tag_from_document(&self, guid: &str, tag: &str) {
        match self {
            Self::Rest(driver) => driver.remove_tag_from_document(guid, tag).await,
            Self::Cosmic(driver) => driver.remove_tag_from_document(guid, tag).await,
        }
    }

    pub async fn document_has_tag(&self, guid: &str, tag: &str) -> bool {
        match self {
            Self::Rest(driver) => driver.document_has_tag(guid, tag).await,
            Self::Cosmic(driver) => driver.document_has_tag(guid, tag).await,
        }
    }

    // -- reading_status --

    pub async fn set_reading_status(&self, guid: &str, status: &str) {
        match self {
            Self::Rest(driver) => driver.set_reading_status(guid, status).await,
            Self::Cosmic(driver) => driver.set_reading_status(guid, status).await,
        }
    }

    pub async fn get_reading_status(&self, guid: &str) -> String {
        match self {
            Self::Rest(driver) => driver.get_reading_status(guid).await,
            Self::Cosmic(driver) => driver.get_reading_status(guid).await,
        }
    }

    // -- documents_list --
    // Reuses `seed_document` (untagged — `seed_tagged_document` minus the
    // tagging step), shared with `tags_list`.

    /// Returns `(file_guid, doc_api_guid, fingerprint)`.
    pub async fn seed_document(&self) -> (String, String, String) {
        match self {
            Self::Rest(driver) => driver.seed_document().await,
            Self::Cosmic(driver) => driver.seed_document().await,
        }
    }

    /// Seeds the second fixture ("Zeta Test Book"). Returns `(file_guid, doc_api_guid, fingerprint)`.
    pub async fn seed_second_document(&self) -> (String, String, String) {
        match self {
            Self::Rest(driver) => driver.seed_second_document().await,
            Self::Cosmic(driver) => driver.seed_second_document().await,
        }
    }

    pub async fn document_is_listed(&self, title: &str) -> bool {
        match self {
            Self::Rest(driver) => driver.document_is_listed(title).await,
            Self::Cosmic(driver) => driver.document_is_listed(title).await,
        }
    }

    // -- admin.scan --

    /// Creates a temp dir with the fixture EPUB, configures it as a scan
    /// directory, and returns the `TempDir` handle (keep alive in world).
    pub async fn prepare_scan_dir(&self) -> tempfile::TempDir {
        match self {
            Self::Rest(driver) => driver.prepare_scan_dir().await,
            Self::Cosmic(driver) => driver.prepare_scan_dir().await,
        }
    }

    /// Triggers a full scan of all configured directories. Returns the number
    /// of documents processed (to be asserted in `Then` steps).
    pub async fn scan_configured(&self) -> u64 {
        match self {
            Self::Rest(driver) => driver.scan_configured().await,
            Self::Cosmic(driver) => driver.scan_configured().await,
        }
    }

    // -- remotes.private_mode --

    pub async fn enable_private_mode(&mut self) {
        match self {
            Self::Rest(driver) => driver.enable_private_mode().await,
            Self::Cosmic(driver) => driver.enable_private_mode().await,
        }
    }

    pub async fn private_mode_is_enabled(&self) -> bool {
        match self {
            Self::Rest(driver) => driver.private_mode_is_enabled().await,
            Self::Cosmic(driver) => driver.private_mode_is_enabled().await,
        }
    }

    // -- admin.check_missing --

    pub async fn check_missing(&self) -> Vec<String> {
        match self {
            Self::Rest(driver) => driver.check_missing().await,
            Self::Cosmic(driver) => driver.check_missing().await,
        }
    }

    // -- sources.delete --

    pub async fn delete_document(&mut self, guid: &str) {
        match self {
            Self::Rest(driver) => driver.delete_document(guid).await,
            Self::Cosmic(driver) => driver.delete_document(guid).await,
        }
    }

    pub async fn file_is_listed(&self, guid: &str) -> bool {
        match self {
            Self::Rest(driver) => driver.file_is_listed(guid).await,
            Self::Cosmic(driver) => driver.file_is_listed(guid).await,
        }
    }

    // -- reading.progress --

    pub async fn set_reading_progress(&self, fingerprint: &str, position: &str, percentage: f64) {
        match self {
            Self::Rest(driver) => {
                driver
                    .set_reading_progress(fingerprint, position, percentage)
                    .await
            }
            Self::Cosmic(driver) => {
                driver
                    .set_reading_progress(fingerprint, position, percentage)
                    .await
            }
        }
    }

    pub async fn get_reading_progress(&self, fingerprint: &str) -> (String, f64) {
        match self {
            Self::Rest(driver) => driver.get_reading_progress(fingerprint).await,
            Self::Cosmic(driver) => driver.get_reading_progress(fingerprint).await,
        }
    }

    // -- documents.detail_view / documents.edit_metadata --

    pub async fn get_document_title(&self, doc_api_guid: &str) -> String {
        match self {
            Self::Rest(driver) => driver.get_document_title(doc_api_guid).await,
            Self::Cosmic(driver) => driver.get_document_title(doc_api_guid).await,
        }
    }

    pub async fn set_document_title(&self, doc_api_guid: &str, title: &str) {
        match self {
            Self::Rest(driver) => driver.set_document_title(doc_api_guid, title).await,
            Self::Cosmic(driver) => driver.set_document_title(doc_api_guid, title).await,
        }
    }

    // -- documents.search / filter_by_status / filter_by_tag --
    // Client-side filtering — no REST surface.

    pub async fn search_returns_document(&self, query: &str, title: &str) -> bool {
        match self {
            Self::Rest(_) => panic!(
                "`documents.search` has no REST surface — run with BDD_DRIVER=cosmic or BDD_DRIVER=pwa"
            ),
            Self::Cosmic(driver) => driver.search_returns_document(query, title).await,
        }
    }

    pub async fn filter_by_status_returns_document(&self, status: &str, title: &str) -> bool {
        match self {
            Self::Rest(_) => panic!(
                "`documents.filter_by_status` has no REST surface — run with BDD_DRIVER=cosmic or BDD_DRIVER=pwa"
            ),
            Self::Cosmic(driver) => {
                driver
                    .filter_by_status_returns_document(status, title)
                    .await
            }
        }
    }

    pub async fn filter_by_tag_returns_document(&self, tag: &str, title: &str) -> bool {
        match self {
            Self::Rest(_) => panic!(
                "`documents.filter_by_tag` has no REST surface — run with BDD_DRIVER=cosmic or BDD_DRIVER=pwa"
            ),
            Self::Cosmic(driver) => driver.filter_by_tag_returns_document(tag, title).await,
        }
    }

    // -- online_library.search --

    pub async fn online_library_search_responds(&self, query: &str) -> bool {
        match self {
            Self::Rest(driver) => driver.online_library_search_responds(query).await,
            Self::Cosmic(driver) => driver.online_library_search_responds(query).await,
        }
    }

    // -- online_library.download_import --

    pub async fn online_library_import_responds(&self) -> bool {
        match self {
            Self::Rest(driver) => driver.online_library_import_responds().await,
            Self::Cosmic(driver) => driver.online_library_import_responds().await,
        }
    }

    // -- sources.send_to_client --

    pub async fn send_document_to_server(&self) -> bool {
        match self {
            Self::Rest(driver) => driver.send_document_to_server().await,
            Self::Cosmic(driver) => driver.send_document_to_server().await,
        }
    }

    // -- documents.select_cover --

    pub async fn set_document_cover_fingerprint(&self, doc_api_guid: &str, fingerprint: &str) {
        match self {
            Self::Rest(driver) => {
                driver
                    .set_document_cover_fingerprint(doc_api_guid, fingerprint)
                    .await
            }
            Self::Cosmic(driver) => {
                driver
                    .set_document_cover_fingerprint(doc_api_guid, fingerprint)
                    .await
            }
        }
    }

    // -- documents.format_picker --

    pub async fn seed_merged_multiformat_document(&self) -> String {
        match self {
            Self::Rest(driver) => driver.seed_merged_multiformat_document().await,
            Self::Cosmic(driver) => driver.seed_merged_multiformat_document().await,
        }
    }

    pub async fn document_has_multiple_formats(&self, doc_api_guid: &str) -> bool {
        match self {
            Self::Rest(driver) => driver.document_has_multiple_formats(doc_api_guid).await,
            Self::Cosmic(driver) => driver.document_has_multiple_formats(doc_api_guid).await,
        }
    }

    // -- documents.pagination --

    pub async fn document_on_first_page(&self, title: &str) -> bool {
        match self {
            Self::Rest(_) => panic!(
                "`documents.pagination` has no REST surface — run with BDD_DRIVER=cosmic or BDD_DRIVER=pwa"
            ),
            Self::Cosmic(driver) => driver.document_on_first_page(title).await,
        }
    }

    // -- documents.filter_by_source --

    pub async fn filter_by_source_returns_document(&self, source_name: &str, title: &str) -> bool {
        match self {
            Self::Rest(_) => panic!(
                "`documents.filter_by_source` has no REST surface — run with BDD_DRIVER=cosmic or BDD_DRIVER=pwa"
            ),
            Self::Cosmic(driver) => {
                driver
                    .filter_by_source_returns_document(source_name, title)
                    .await
            }
        }
    }

    // -- documents.merge --

    pub async fn merge_documents(&self, winner_guid: &str, loser_guid: &str) {
        match self {
            Self::Rest(driver) => driver.merge_documents(winner_guid, loser_guid).await,
            Self::Cosmic(driver) => driver.merge_documents(winner_guid, loser_guid).await,
        }
    }

    pub async fn document_count(&self) -> usize {
        match self {
            Self::Rest(driver) => driver.document_count().await,
            Self::Cosmic(driver) => driver.document_count().await,
        }
    }

    // -- documents.sort --

    pub async fn sorted_document_titles_ascending(&self) -> Vec<String> {
        match self {
            Self::Rest(_) => panic!(
                "`documents.sort` has no REST surface — run with BDD_DRIVER=cosmic or BDD_DRIVER=pwa"
            ),
            Self::Cosmic(driver) => driver.sorted_document_titles_ascending().await,
        }
    }

    // -- documents.batch_tag --

    pub async fn batch_add_tag(&self, doc_api_guid: &str, tag: &str) {
        match self {
            Self::Rest(_) => panic!(
                "`documents.batch_tag` has no REST surface — run with BDD_DRIVER=cosmic or BDD_DRIVER=pwa"
            ),
            Self::Cosmic(driver) => driver.batch_add_tag(doc_api_guid, tag).await,
        }
    }

    // -- reading.pdf_viewer --

    pub async fn seed_pdf_document(&self) -> (String, String, String) {
        match self {
            Self::Rest(driver) => driver.seed_pdf_document().await,
            Self::Cosmic(driver) => driver.seed_pdf_document().await,
        }
    }

    pub fn pdf_opens_successfully(&self) -> bool {
        match self {
            Self::Rest(_) => panic!(
                "`reading.pdf_viewer` has no REST surface — run with BDD_DRIVER=cosmic or BDD_DRIVER=pwa"
            ),
            Self::Cosmic(driver) => driver.pdf_opens_successfully(),
        }
    }

    // -- app.epub_viewer_choice --

    pub async fn set_epub_viewer_choice(&mut self, choice: &str) {
        match self {
            Self::Rest(_) => {
                panic!("`app.epub_viewer_choice` has no REST surface — run with BDD_DRIVER=cosmic")
            }
            Self::Cosmic(driver) => driver.set_epub_viewer_choice(choice).await,
        }
    }

    pub fn epub_viewer_choice(&self) -> &str {
        match self {
            Self::Rest(_) => {
                panic!("`app.epub_viewer_choice` has no REST surface — run with BDD_DRIVER=cosmic")
            }
            Self::Cosmic(driver) => driver.epub_viewer_choice(),
        }
    }

    // -- documents.cover_display --

    pub async fn seed_cover_document(&self) -> (String, String, String) {
        match self {
            Self::Rest(driver) => driver.seed_cover_document().await,
            Self::Cosmic(driver) => driver.seed_cover_document().await,
        }
    }

    pub async fn document_has_cover(&self, doc_api_guid: &str) -> bool {
        match self {
            Self::Rest(driver) => driver.document_has_cover(doc_api_guid).await,
            Self::Cosmic(driver) => driver.document_has_cover(doc_api_guid).await,
        }
    }

    // -- reading.image_viewer --

    pub fn image_viewer_opens_successfully(&self) -> bool {
        match self {
            Self::Rest(_) => {
                panic!("`reading.image_viewer` has no REST surface — run with BDD_DRIVER=cosmic")
            }
            Self::Cosmic(driver) => driver.image_viewer_opens_successfully(),
        }
    }

    // -- reading.epub_viewer --

    pub async fn epub_opens_successfully(&self, doc_api_guid: &str) -> bool {
        match self {
            Self::Rest(_) => panic!(
                "`reading.epub_viewer` has no REST surface — run with BDD_DRIVER=cosmic or BDD_DRIVER=pwa"
            ),
            Self::Cosmic(driver) => driver.epub_opens_successfully(doc_api_guid).await,
        }
    }
}

fn env_name() -> &'static str {
    match std::env::var("BDD_DRIVER").as_deref() {
        Ok("cosmic") => "cosmic",
        _ => "rest",
    }
}
