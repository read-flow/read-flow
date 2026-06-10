//! Drives Gherkin steps against a real `read-flow-cli serve` process over
//! HTTP — the most direct mapping for REST-surfaced behavior. Reuses
//! `read_flow_core::test_support::TestServer`, the same boot recipe the PWA
//! Playwright harness follows independently (see `pwa/e2e/support/server.ts`).

use read_flow_core::test_support::TestServer;

use crate::bdd::fixtures::sample_cover_epub_path;
use crate::bdd::fixtures::sample_epub_path;
use crate::bdd::fixtures::sample_pdf_path;
use crate::bdd::fixtures::sample2_epub_path;

pub const USER: &str = "alice";
pub const PASSWORD: &str = "correct-horse";

pub struct RestDriver {
    server: TestServer,
    client: reqwest::Client,
}

impl RestDriver {
    pub async fn new() -> Self {
        Self {
            server: TestServer::spawn(USER, PASSWORD).await,
            client: reqwest::Client::new(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.server.base_url
    }

    /// `GET /status` with the server's configured credentials — REST has no
    /// "add remote" concept, so the natural mapping for "a remote reports as
    /// reachable" is "the server's own status endpoint accepts these creds".
    pub async fn status_with(&self, user: &str, password: &str) -> bool {
        self.client
            .get(format!("{}/status", self.server.base_url))
            .basic_auth(user, Some(password))
            .send()
            .await
            .map(|response| response.status().is_success())
            .unwrap_or(false)
    }

    pub async fn status_is_healthy(&self) -> bool {
        self.status_with(&self.server.user, &self.server.password)
            .await
    }

    /// Owner-only `/settings` GET/PUT, manipulated as raw JSON — no need to
    /// duplicate the server's private `ServerSettingsDto` shape for one field.
    async fn get_settings(&self) -> serde_json::Value {
        self.client
            .get(format!("{}/settings", self.server.base_url))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .send()
            .await
            .expect("GET /settings")
            .json()
            .await
            .expect("parse settings JSON")
    }

    async fn put_settings(&self, dto: serde_json::Value) {
        let response = self
            .client
            .put(format!("{}/settings", self.server.base_url))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .json(&dto)
            .send()
            .await
            .expect("PUT /settings");
        assert!(
            response.status().is_success(),
            "PUT /settings failed: {}",
            response.status()
        );
    }

    pub async fn enable_dry_run_and_save(&self) {
        let mut dto = self.get_settings().await;
        dto["dry_run"] = serde_json::Value::Bool(true);
        self.put_settings(dto).await;
    }

    pub async fn dry_run_is_enabled(&self) -> bool {
        self.get_settings().await["dry_run"]
            .as_bool()
            .expect("dry_run is a bool")
    }

    /// `PUT /scan-directories` with a flattened `ScanDirectoryEntry` — raw
    /// JSON, same rationale as `get_settings`/`put_settings`.
    pub async fn add_scan_directory(&self, path: &str) {
        let response = self
            .client
            .put(format!("{}/scan-directories", self.server.base_url))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .json(&serde_json::json!({
                "path": path,
                "action": "Scan",
                "tags": [],
                "inherit": false,
            }))
            .send()
            .await
            .expect("PUT /scan-directories");
        assert!(
            response.status().is_success(),
            "PUT /scan-directories failed: {}",
            response.status()
        );
    }

    pub async fn scan_directory_is_listed(&self, path: &str) -> bool {
        let entries: Vec<serde_json::Value> = self
            .client
            .get(format!("{}/scan-directories", self.server.base_url))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .send()
            .await
            .expect("GET /scan-directories")
            .json()
            .await
            .expect("parse scan-directories JSON");
        entries.iter().any(|entry| entry["path"] == path)
    }

    /// `POST /users` with a `CreateUserRequest`-shaped body — raw JSON, same
    /// rationale as `add_scan_directory` (the DTO is a private server type).
    pub async fn add_user(&self, user_id: &str, password: &str) {
        let response = self
            .client
            .post(format!("{}/users", self.server.base_url))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .json(&serde_json::json!({
                "user_id": user_id,
                "password": password,
                "roles": [],
            }))
            .send()
            .await
            .expect("POST /users");
        assert!(
            response.status().is_success(),
            "POST /users failed: {}",
            response.status()
        );
    }

    pub async fn user_is_listed(&self, user_id: &str) -> bool {
        let entries: Vec<serde_json::Value> = self
            .client
            .get(format!("{}/users", self.server.base_url))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .send()
            .await
            .expect("GET /users")
            .json()
            .await
            .expect("parse users JSON");
        entries.iter().any(|entry| entry["user_id"] == user_id)
    }

    /// Uploads the shared `sample.epub` fixture via `POST /files` (multipart,
    /// field name `file` — confirmed against the PWA's `client.ts` upload),
    /// returning the resulting file's guid. The only seeding path available
    /// out-of-process: `TestServer` exposes HTTP only, no DB pool (see
    /// `tags_list.feature`'s doc comment).
    /// Returns `(file_guid, doc_api_guid, fingerprint)`.
    pub async fn seed_document(&self) -> (String, String, String) {
        let bytes = std::fs::read(sample_epub_path()).expect("read fixture epub");
        let part = reqwest::multipart::Part::bytes(bytes)
            .file_name("sample.epub")
            .mime_str("application/epub+zip")
            .expect("mime");
        let form = reqwest::multipart::Form::new().part("file", part);
        let file: serde_json::Value = self
            .client
            .post(format!("{}/files", self.server.base_url))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .multipart(form)
            .send()
            .await
            .expect("POST /files")
            .json()
            .await
            .expect("parse uploaded file JSON");
        let file_guid = file["guid"].as_str().expect("guid field").to_string();
        let doc_api_guid = file["document_guid"]
            .as_str()
            .expect("document_guid field")
            .to_string();
        let fingerprint = file["fingerprint"]
            .as_str()
            .expect("fingerprint field")
            .to_string();
        (file_guid, doc_api_guid, fingerprint)
    }

    /// Seeds the second fixture ("Zeta Test Book"). Returns `(file_guid, doc_api_guid, fingerprint)`.
    pub async fn seed_second_document(&self) -> (String, String, String) {
        let bytes = std::fs::read(sample2_epub_path()).expect("read fixture sample2.epub");
        let part = reqwest::multipart::Part::bytes(bytes)
            .file_name("sample2.epub")
            .mime_str("application/epub+zip")
            .expect("mime");
        let form = reqwest::multipart::Form::new().part("file", part);
        let file: serde_json::Value = self
            .client
            .post(format!("{}/files", self.server.base_url))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .multipart(form)
            .send()
            .await
            .expect("POST /files (sample2)")
            .json()
            .await
            .expect("parse uploaded file JSON");
        let file_guid = file["guid"].as_str().expect("guid field").to_string();
        let doc_api_guid = file["document_guid"]
            .as_str()
            .expect("document_guid field")
            .to_string();
        let fingerprint = file["fingerprint"]
            .as_str()
            .expect("fingerprint field")
            .to_string();
        (file_guid, doc_api_guid, fingerprint)
    }

    /// Merges `loser_guid` into `winner_guid` via POST /documents/merge.
    pub async fn merge_documents(&self, winner_guid: &str, loser_guid: &str) {
        let response = self
            .client
            .post(format!("{}/documents/merge", self.server.base_url))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .json(&serde_json::json!({
                "winner_guid": winner_guid,
                "loser_guids": [loser_guid],
            }))
            .send()
            .await
            .expect("POST /documents/merge");
        assert!(
            response.status().is_success(),
            "POST /documents/merge failed: {}",
            response.status()
        );
    }

    /// Returns the total number of documents via GET /documents.
    pub async fn document_count(&self) -> usize {
        let docs: Vec<serde_json::Value> = self
            .client
            .get(format!("{}/documents", self.server.base_url))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .send()
            .await
            .expect("GET /documents")
            .json()
            .await
            .expect("parse documents JSON");
        docs.len()
    }

    pub async fn add_tag_to_document(&self, guid: &str, tag: &str) {
        let response = self
            .client
            .post(format!("{}/files/{}/tags", self.server.base_url, guid))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .json(&serde_json::json!([tag]))
            .send()
            .await
            .expect("POST /files/<guid>/tags");
        assert!(
            response.status().is_success(),
            "POST /files/{guid}/tags failed: {}",
            response.status()
        );
    }

    pub async fn remove_tag_from_document(&self, guid: &str, tag: &str) {
        let response = self
            .client
            .delete(format!("{}/files/{}/tags", self.server.base_url, guid))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .json(&serde_json::json!([tag]))
            .send()
            .await
            .expect("DELETE /files/<guid>/tags");
        assert!(
            response.status().is_success(),
            "DELETE /files/{guid}/tags failed: {}",
            response.status()
        );
    }

    pub async fn document_has_tag(&self, guid: &str, tag: &str) -> bool {
        let tags: Vec<String> = self
            .client
            .get(format!("{}/files/{}/tags", self.server.base_url, guid))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .send()
            .await
            .expect("GET /files/<guid>/tags")
            .json()
            .await
            .expect("parse tags JSON");
        tags.iter().any(|t| t == tag)
    }

    /// Gets a file's fingerprint via `GET /files/<guid>`, then sets reading
    /// status via `PUT /reading-state/<fingerprint>/status`.
    pub async fn set_reading_status(&self, guid: &str, status: &str) {
        let file: serde_json::Value = self
            .client
            .get(format!("{}/files/{}", self.server.base_url, guid))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .send()
            .await
            .expect("GET /files/<guid>")
            .json()
            .await
            .expect("parse file JSON");
        let fingerprint = file["fingerprint"].as_str().expect("fingerprint field");
        let response = self
            .client
            .put(format!(
                "{}/reading-state/{}/status",
                self.server.base_url, fingerprint
            ))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .json(&serde_json::json!({ "status": status }))
            .send()
            .await
            .expect("PUT /reading-state/<fp>/status");
        assert!(
            response.status().is_success(),
            "PUT /reading-state/{fingerprint}/status failed: {}",
            response.status()
        );
    }

    pub async fn get_reading_status(&self, guid: &str) -> String {
        let file: serde_json::Value = self
            .client
            .get(format!("{}/files/{}", self.server.base_url, guid))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .send()
            .await
            .expect("GET /files/<guid>")
            .json()
            .await
            .expect("parse file JSON");
        file["status"].as_str().expect("status field").to_string()
    }

    /// `seed_document` plus tagging it. Returns `(file_guid, doc_api_guid, fingerprint)`.
    pub async fn seed_tagged_document(&self, tag: &str) -> (String, String, String) {
        let (guid, doc_guid, fingerprint) = self.seed_document().await;
        let response = self
            .client
            .post(format!("{}/files/{}/tags", self.server.base_url, guid))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .json(&serde_json::json!([tag]))
            .send()
            .await
            .expect("POST /files/<guid>/tags");
        assert!(
            response.status().is_success(),
            "POST /files/{guid}/tags failed: {}",
            response.status()
        );
        (guid, doc_guid, fingerprint)
    }

    pub async fn prepare_scan_dir(&self) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("temp scan dir");
        std::fs::copy(sample_epub_path(), dir.path().join("sample.epub")).expect("copy fixture");
        self.add_scan_directory(&dir.path().to_string_lossy()).await;
        dir
    }

    pub async fn scan_configured(&self) -> u64 {
        let summary: serde_json::Value = self
            .client
            .post(format!("{}/scan", self.server.base_url))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .send()
            .await
            .expect("POST /scan")
            .json()
            .await
            .expect("parse scan summary JSON");
        summary["processed"].as_u64().expect("processed field")
    }

    pub async fn get_document_title(&self, doc_api_guid: &str) -> String {
        let doc: serde_json::Value = self
            .client
            .get(format!(
                "{}/documents/{}",
                self.server.base_url, doc_api_guid
            ))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .send()
            .await
            .expect("GET /documents/<guid>")
            .json()
            .await
            .expect("parse document JSON");
        doc["metadata"]["title"]
            .as_str()
            .expect("title field")
            .to_string()
    }

    pub async fn set_document_title(&self, doc_api_guid: &str, title: &str) {
        let response = self
            .client
            .put(format!(
                "{}/documents/{}/metadata",
                self.server.base_url, doc_api_guid
            ))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .json(&serde_json::json!({
                "title": title,
                "document_type": null,
                "subtitle": null,
                "authors": null,
                "description": null,
                "language": null,
                "publisher": null,
                "identifier": null,
                "date": null,
                "subject": null,
                "selected_cover_fingerprint": null,
            }))
            .send()
            .await
            .expect("PUT /documents/<guid>/metadata");
        assert!(
            response.status().is_success(),
            "PUT /documents/{doc_api_guid}/metadata failed: {}",
            response.status()
        );
    }

    pub async fn enable_private_mode(&self) {
        let mut dto = self.get_settings().await;
        dto["private_mode"] = serde_json::Value::Bool(true);
        self.put_settings(dto).await;
    }

    pub async fn private_mode_is_enabled(&self) -> bool {
        self.get_settings().await["private_mode"]
            .as_bool()
            .expect("private_mode is a bool")
    }

    pub async fn check_missing(&self) -> Vec<String> {
        let resp: serde_json::Value = self
            .client
            .post(format!(
                "{}/maintenance/check-missing",
                self.server.base_url
            ))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .send()
            .await
            .expect("POST /maintenance/check-missing")
            .json()
            .await
            .expect("parse check-missing JSON");
        resp["missing"]
            .as_array()
            .expect("missing array")
            .iter()
            .map(|v| v.as_str().expect("path string").to_string())
            .collect()
    }

    pub async fn delete_document(&self, guid: &str) {
        let response = self
            .client
            .delete(format!("{}/files/{}", self.server.base_url, guid))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .send()
            .await
            .expect("DELETE /files/<guid>");
        assert!(
            response.status().is_success(),
            "DELETE /files/{guid} failed: {}",
            response.status()
        );
    }

    pub async fn file_is_listed(&self, guid: &str) -> bool {
        let files: Vec<serde_json::Value> = self
            .client
            .get(format!("{}/files", self.server.base_url))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .send()
            .await
            .expect("GET /files")
            .json()
            .await
            .expect("parse files JSON");
        files.iter().any(|f| f["guid"] == guid)
    }

    pub async fn set_reading_progress(&self, fingerprint: &str, position: &str, percentage: f64) {
        let response = self
            .client
            .put(format!("{}/reading-state", self.server.base_url))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .json(&serde_json::json!({
                "fingerprint": fingerprint,
                "status": 1,
                "position": position,
                "percentage": percentage,
                "last_updated": "",
                "status_updated_at": "",
            }))
            .send()
            .await
            .expect("PUT /reading-state");
        assert!(
            response.status().is_success(),
            "PUT /reading-state failed: {}",
            response.status()
        );
    }

    pub async fn get_reading_progress(&self, fingerprint: &str) -> (String, f64) {
        let state: serde_json::Value = self
            .client
            .get(format!(
                "{}/reading-state/{}",
                self.server.base_url, fingerprint
            ))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .send()
            .await
            .expect("GET /reading-state/<fp>")
            .json()
            .await
            .expect("parse reading-state JSON");
        let position = state["position"].as_str().expect("position").to_string();
        let percentage = state["percentage"].as_f64().expect("percentage");
        (position, percentage)
    }

    /// `GET /documents` — the same listing the PWA's library page and
    /// COSMIC's `DocumentListPage` aggregate over.
    pub async fn document_is_listed(&self, title: &str) -> bool {
        let documents: Vec<serde_json::Value> = self
            .client
            .get(format!("{}/documents", self.server.base_url))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .send()
            .await
            .expect("GET /documents")
            .json()
            .await
            .expect("parse documents JSON");
        documents
            .iter()
            .any(|doc| doc["metadata"]["title"] == title)
    }

    pub async fn tag_is_listed(&self, tag: &str) -> bool {
        let tags: Vec<String> = self
            .client
            .get(format!("{}/files/tags", self.server.base_url))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .send()
            .await
            .expect("GET /files/tags")
            .json()
            .await
            .expect("parse tags JSON");
        tags.iter().any(|t| t == tag)
    }

    // -- reading.pdf_viewer --

    /// Uploads the PDF fixture via `POST /files`.
    /// Returns `(file_guid, doc_api_guid, fingerprint)`.
    pub async fn seed_pdf_document(&self) -> (String, String, String) {
        let bytes = std::fs::read(sample_pdf_path()).expect("read PDF fixture");
        let part = reqwest::multipart::Part::bytes(bytes)
            .file_name("sample.pdf")
            .mime_str("application/pdf")
            .expect("mime");
        let form = reqwest::multipart::Form::new().part("file", part);
        let file: serde_json::Value = self
            .client
            .post(format!("{}/files", self.server.base_url))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .multipart(form)
            .send()
            .await
            .expect("POST /files (pdf)")
            .json()
            .await
            .expect("parse uploaded file JSON");
        let file_guid = file["guid"].as_str().expect("guid field").to_string();
        let doc_api_guid = file["document_guid"]
            .as_str()
            .expect("document_guid field")
            .to_string();
        let fingerprint = file["fingerprint"]
            .as_str()
            .expect("fingerprint field")
            .to_string();
        (file_guid, doc_api_guid, fingerprint)
    }

    // -- documents.cover_display --

    /// Uploads the cover fixture (`sample_cover.epub`) via `POST /files`.
    /// Returns `(file_guid, doc_api_guid, fingerprint)`.
    pub async fn seed_cover_document(&self) -> (String, String, String) {
        let bytes = std::fs::read(sample_cover_epub_path()).expect("read cover fixture epub");
        let part = reqwest::multipart::Part::bytes(bytes)
            .file_name("sample_cover.epub")
            .mime_str("application/epub+zip")
            .expect("mime");
        let form = reqwest::multipart::Form::new().part("file", part);
        let file: serde_json::Value = self
            .client
            .post(format!("{}/files", self.server.base_url))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .multipart(form)
            .send()
            .await
            .expect("POST /files (cover)")
            .json()
            .await
            .expect("parse uploaded file JSON");
        let file_guid = file["guid"].as_str().expect("guid field").to_string();
        let doc_api_guid = file["document_guid"]
            .as_str()
            .expect("document_guid field")
            .to_string();
        let fingerprint = file["fingerprint"]
            .as_str()
            .expect("fingerprint field")
            .to_string();
        (file_guid, doc_api_guid, fingerprint)
    }

    /// `GET /documents/<guid>/cover` — returns `true` when the server
    /// responds with 200 (cover image present).
    pub async fn document_has_cover(&self, doc_api_guid: &str) -> bool {
        self.client
            .get(format!(
                "{}/documents/{}/cover",
                self.server.base_url, doc_api_guid
            ))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }
}
