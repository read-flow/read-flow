// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::Infallible;
use std::env;
use std::ffi::OsStr;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use base64::Engine;
use futures::StreamExt;
use reqwest::Client;
use reqwest::RequestBuilder;
use reqwest::StatusCode;
use reqwest::Url;
use reqwest::header;
use tokio::fs;
use tokio::sync::Mutex;

use crate::Builder;
use crate::api::ApiDocument;
use crate::api::DocumentMeta;
use crate::api::File;
use crate::api::FileDataSource;
use crate::api::ReadingState;
use crate::api::ReadingStatus;
use crate::api::Status;
use crate::extension_of;
use crate::to_unique_file;

/// A Bearer access token cached until shortly before it expires.
struct CachedToken {
    value: String,
    expires_at: Instant,
}

/// Subset of the `/oauth/token` response we need.
#[derive(serde::Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
}

#[derive(Clone)]
pub struct FilesClient {
    pub base_url: Url,
    user_id: String,
    passphrase: String,
    private_mode: bool,
    client: Client,
    /// Cached Bearer token, shared across clones of this client.
    token: Arc<Mutex<Option<CachedToken>>>,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("invalid URL: {0}")]
    Url(#[from] url::ParseError),
    #[error("HTTP request failed: {0}")]
    Http(#[source] Arc<reqwest::Error>),
    #[error("file system error: {0}")]
    IO(#[source] Arc<io::Error>),
    #[error("file name is missing extension: {0}")]
    MissingExtension(String),
    #[error("the source file doesn't exist: {0}")]
    SourceDoesntExist(PathBuf),
    #[error("programmer error")]
    Unexpected(Infallible),
    #[error("invalid path: {0}")]
    InvalidFile(PathBuf),
}

/// Warn when credentials would be sent over plaintext HTTP to a non-loopback
/// host. Loopback (localhost / 127.0.0.1 / ::1) is fine for same-machine use.
fn warn_if_cleartext(base_url: &Url) {
    if base_url.scheme() != "https" {
        let loopback = match base_url.host() {
            Some(url::Host::Domain(h)) => h == "localhost",
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            None => false,
        };
        if !loopback {
            tracing::warn!(
                "credentials will be sent over plaintext HTTP to {base_url} — use HTTPS \
                 (see the deployment docs) to avoid interception"
            );
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Self {
        Error::Http(error.into())
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Error::IO(Arc::new(error))
    }
}

impl FilesClient {
    pub fn new<U: Into<Url>>(
        base_url: U,
        user_id: String,
        passphrase: String,
        private_mode: bool,
    ) -> Result<Self, Infallible> {
        let base_url = base_url.into();
        warn_if_cleartext(&base_url);
        let result = Self {
            base_url,
            user_id,
            passphrase,
            private_mode,
            client: Client::new(),
            token: Arc::new(Mutex::new(None)),
        };
        Ok(result)
    }

    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    fn basic_header(&self) -> String {
        // HTTP Basic with user_id:passphrase — used to obtain a token, and as a
        // fallback when the server has no token endpoint.
        let credentials = format!("{}:{}", self.user_id, self.passphrase);
        let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
        format!("Basic {}", encoded)
    }

    /// The `Authorization` header value to use: a cached Bearer token if valid,
    /// otherwise obtain one via `/oauth/token`, falling back to Basic.
    async fn auth_header(&self) -> String {
        if let Some(bearer) = self.cached_bearer().await {
            return bearer;
        }
        self.fetch_token()
            .await
            .unwrap_or_else(|| self.basic_header())
    }

    async fn cached_bearer(&self) -> Option<String> {
        let guard = self.token.lock().await;
        guard
            .as_ref()
            .filter(|t| t.expires_at > Instant::now())
            .map(|t| format!("Bearer {}", t.value))
    }

    /// Exchange Basic credentials for a Bearer token and cache it. Returns
    /// `None` if the server has no `/oauth/token` (older server) or on any
    /// failure, so the caller falls back to Basic.
    async fn fetch_token(&self) -> Option<String> {
        let url = self.base_url.join("oauth/token").ok()?;
        let response = self
            .client
            .post(url)
            .header(header::AUTHORIZATION, self.basic_header())
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body("grant_type=password")
            .send()
            .await
            .ok()?;
        if !response.status().is_success() {
            return None;
        }
        let body: TokenResponse = response.json().await.ok()?;
        // Refresh a little early to avoid racing the expiry.
        let expires_at = Instant::now() + Duration::from_secs(body.expires_in.saturating_sub(30));
        let bearer = format!("Bearer {}", body.access_token);
        *self.token.lock().await = Some(CachedToken {
            value: body.access_token,
            expires_at,
        });
        Some(bearer)
    }

    async fn invalidate_token(&self) {
        *self.token.lock().await = None;
    }

    /// Send `builder` with the current auth header. If the server replies 401
    /// (e.g. the token expired or the server restarted with a new secret),
    /// drop the cached token and retry once with a fresh one.
    async fn send(&self, builder: RequestBuilder) -> Result<reqwest::Response, Error> {
        let retry = builder.try_clone();
        let response = builder
            .header(header::AUTHORIZATION, self.auth_header().await)
            .send()
            .await?;
        if response.status() == StatusCode::UNAUTHORIZED
            && let Some(retry) = retry
        {
            self.invalidate_token().await;
            return Ok(retry
                .header(header::AUTHORIZATION, self.auth_header().await)
                .send()
                .await?);
        }
        Ok(response)
    }

    async fn get_json<T>(&self, relative_url: &str) -> Result<T, Error>
    where
        T: for<'a> serde::Deserialize<'a>,
    {
        let builder = self
            .client
            .get(self.base_url.join(relative_url)?)
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON))
            .apply_if(self.private_mode, |req| {
                req.header("x-private-mode", "true")
            });
        let result = self.send(builder).await?.json().await?;
        Ok(result)
    }

    fn get_target_file(filename: &str) -> Result<PathBuf, Error> {
        let mut file_path: PathBuf = filename.parse().map_err(Error::Unexpected)?;

        let orig_extension =
            extension_of(filename).ok_or(Error::MissingExtension(filename.to_string()))?;
        to_unique_file(&mut file_path, orig_extension);
        Ok(file_path)
    }

    pub async fn download_file(&self, guid: &str, filename: &Path) -> Result<PathBuf, Error> {
        let builder = self
            .client
            .get(self.base_url.join(&format!(
                "files/{guid}/download-as/{}",
                filename.file_name().and_then(OsStr::to_str).unwrap()
            ))?)
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON));
        let response = self.send(builder).await?;

        let mut bytes = response.bytes_stream();

        let target_filename = Self::get_target_file(&format!("{}", filename.display()))?;
        let mut target_file = fs::File::create(&target_filename).await?;

        while let Some(item) = bytes.next().await {
            tokio::io::copy(&mut item?.as_ref(), &mut target_file).await?;
        }

        Ok(target_filename)
    }

    pub async fn upload_file(&self, filename: &Path) -> Result<File, Error> {
        if !filename.exists() {
            return Err(Error::SourceDoesntExist(filename.to_path_buf()));
        }

        let form = reqwest::multipart::Form::new()
            .file("file", filename)
            .await?;

        let builder = self
            .client
            .post(self.base_url.join("/files")?)
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON))
            .multipart(form);
        let response = self.send(builder).await?;

        let result = response.json().await?;

        Ok(result)
    }
}

#[async_trait::async_trait]
impl FileDataSource for FilesClient {
    type Error = Error;

    fn display_name(&self) -> String {
        let host = self.base_url.host_str().unwrap_or("Unknown");
        format!("Remote: {host}")
    }

    async fn status(&self) -> Result<Status, Error> {
        let server_status: Status = self.get_json("status").await?;
        let status = Status {
            identifier: "client".to_string(),
            nested_checks: vec![server_status],
            ..Default::default()
        };
        Ok(status)
    }

    async fn get_files(&self) -> Result<Vec<File>, Error> {
        self.get_json("files").await
    }

    async fn get_files_tags(&self) -> Result<Vec<String>, Error> {
        self.get_json("files/tags").await
    }

    async fn get_file(&self, guid: &str) -> Result<Option<File>, Error> {
        self.get_json(&format!("files/{guid}")).await
    }

    async fn update_file(&self, file: File) -> Result<(), Error> {
        let builder = self
            .client
            .put(self.base_url.join("/files")?)
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON))
            .json(&file);
        let response = self.send(builder).await?;

        response.error_for_status_ref()?;

        Ok(())
    }

    async fn get_file_tags(&self, guid: &str) -> Result<Vec<String>, Error> {
        self.get_json(&format!("files/{guid}/tags")).await
    }

    async fn add_file_tags(&self, guid: &str, tags: Vec<String>) -> Result<Vec<String>, Error> {
        let builder = self
            .client
            .post(self.base_url.join(&format!("/files/{guid}/tags"))?)
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON))
            .json(&tags);
        let response = self.send(builder).await?;

        let result = response.json().await?;

        Ok(result)
    }

    async fn delete_file_tags(&self, guid: &str, tags: Vec<String>) -> Result<(), Error> {
        let builder = self
            .client
            .delete(self.base_url.join(&format!("/files/{guid}/tags"))?)
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON))
            .json(&tags);
        let response = self.send(builder).await?;

        let _result: Vec<String> = response.json().await?;

        Ok(())
    }

    async fn open_file(&self, file: File) -> Result<(), Error> {
        let tempdir = env::temp_dir().join("read-flow");

        if !tempdir.exists() {
            tokio::fs::create_dir(&tempdir).await?;
        }

        let file_path = PathBuf::from(file.path);
        let mut filename = tempdir.join(PathBuf::from(file_path.file_name().unwrap()));

        if !filename.exists()
            || crate::sha256_of_file(&filename)
                .await
                .map_err(|e| Error::IO(Arc::new(e)))?
                != file.fingerprint
        {
            filename = self.download_file(&file.guid, &filename).await?;
        }

        open::that_detached(&filename).map_err(|e| Error::IO(Arc::new(e)))
    }

    async fn delete_file(&self, file: File) -> Result<(), Error> {
        let builder = self
            .client
            .delete(self.base_url.join(&format!("files/{}", file.guid))?)
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON));
        let response = self.send(builder).await?;

        // Check if the request was successful
        response.error_for_status_ref()?;

        Ok(())
    }

    async fn import_file(&self, path: &Path) -> Result<File, Error> {
        self.upload_file(path).await
    }

    async fn get_reading_state(&self, fingerprint: &str) -> Result<Option<ReadingState>, Error> {
        let builder = self
            .client
            .get(
                self.base_url
                    .join(&format!("reading-state/{fingerprint}"))?,
            )
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON));
        let response = self.send(builder).await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        let result = response.json().await?;
        Ok(Some(result))
    }

    async fn upsert_reading_state(&self, state: ReadingState) -> Result<ReadingState, Error> {
        let builder = self
            .client
            .put(self.base_url.join("reading-state")?)
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON))
            .json(&state);
        let response = self.send(builder).await?;

        response.error_for_status_ref()?;
        Ok(response.json().await?)
    }

    async fn update_reading_status(
        &self,
        fingerprint: &str,
        status: ReadingStatus,
    ) -> Result<(), Error> {
        #[derive(serde::Serialize)]
        struct Req {
            status: ReadingStatus,
        }
        let builder = self
            .client
            .put(
                self.base_url
                    .join(&format!("reading-state/{fingerprint}/status"))?,
            )
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON))
            .json(&Req { status });
        let response = self.send(builder).await?;

        response.error_for_status_ref()?;
        Ok(())
    }
}

impl FilesClient {
    pub async fn get_documents(&self) -> Result<Vec<ApiDocument>, Error> {
        self.get_json("documents").await
    }

    pub async fn get_document(&self, guid: &str) -> Result<Option<ApiDocument>, Error> {
        let builder = self
            .client
            .get(self.base_url.join(&format!("documents/{guid}"))?)
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON));
        let response = self.send(builder).await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        Ok(Some(response.json().await?))
    }

    pub async fn update_document_metadata(
        &self,
        guid: &str,
        meta: DocumentMeta,
    ) -> Result<ApiDocument, Error> {
        let builder = self
            .client
            .put(self.base_url.join(&format!("documents/{guid}/metadata"))?)
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON))
            .json(&meta);
        let response = self.send(builder).await?;
        response.error_for_status_ref()?;
        Ok(response.json().await?)
    }

    pub async fn merge_documents(
        &self,
        winner_guid: &str,
        loser_guids: &[String],
    ) -> Result<(), Error> {
        let req = crate::api::MergeDocumentsRequest {
            winner_guid: winner_guid.to_string(),
            loser_guids: loser_guids.to_vec(),
        };
        let builder = self
            .client
            .post(self.base_url.join("documents/merge")?)
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON))
            .json(&req);
        let response = self.send(builder).await?;
        response.error_for_status_ref()?;
        Ok(())
    }

    pub async fn ensure_document_for_file(&self, file_guid: &str) -> Result<ApiDocument, Error> {
        let builder = self
            .client
            .post(self.base_url.join(&format!("files/{file_guid}/document"))?)
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON));
        let response = self.send(builder).await?;
        response.error_for_status_ref()?;
        Ok(response.json().await?)
    }
}
