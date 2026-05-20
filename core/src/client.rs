use std::convert::Infallible;
use std::env;
use std::ffi::OsStr;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::sync::Arc;

use base64::Engine;
use futures::StreamExt;
use reqwest::Client;
use reqwest::Url;
use reqwest::header;
use tokio::fs;
use tokio::process::Command;

use crate::Builder;
use crate::api::ApiDocument;
use crate::api::DocumentMeta;
use crate::api::File;
use crate::api::FileDataSource;
use crate::api::ReadingProgress;
use crate::api::Status;
use crate::db::models::ContentMetadata;
use crate::extension_of;
use crate::to_unique_file;

#[derive(Clone)]
pub struct FilesClient {
    pub base_url: Url,
    user_id: String,
    passphrase: String,
    private_mode: bool,
    client: Client,
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
        let result = Self {
            base_url: base_url.into(),
            user_id,
            passphrase,
            private_mode,
            client: Client::new(),
        };
        Ok(result)
    }

    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    fn get_auth_header(&self) -> String {
        // Use Basic authentication with user_id:passphrase
        let credentials = format!("{}:{}", self.user_id, self.passphrase);
        let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
        format!("Basic {}", encoded)
    }

    async fn get_json<T>(&self, relative_url: &str) -> Result<T, Error>
    where
        T: for<'a> serde::Deserialize<'a>,
    {
        let result = self
            .client
            .get(self.base_url.join(relative_url)?)
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON))
            .header(header::AUTHORIZATION, self.get_auth_header())
            .apply_if(self.private_mode, |req| {
                req.header("x-private-mode", "true")
            })
            .send()
            .await?
            .json()
            .await?;
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
        let response = self
            .client
            .get(self.base_url.join(&format!(
                "files/{guid}/download-as/{}",
                filename.file_name().and_then(OsStr::to_str).unwrap()
            ))?)
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON))
            .header(header::AUTHORIZATION, self.get_auth_header())
            .send()
            .await?;

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

        let response = self
            .client
            .post(self.base_url.join("/files")?)
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON))
            .header(header::AUTHORIZATION, self.get_auth_header())
            .multipart(form)
            .send()
            .await?;

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
        let response = self
            .client
            .put(self.base_url.join("/files")?)
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON))
            .header(header::AUTHORIZATION, self.get_auth_header())
            .json(&file)
            .send()
            .await?;

        response.error_for_status_ref()?;

        Ok(())
    }

    async fn get_file_tags(&self, guid: &str) -> Result<Vec<String>, Error> {
        self.get_json(&format!("files/{guid}/tags")).await
    }

    async fn add_file_tags(&self, guid: &str, tags: Vec<String>) -> Result<Vec<String>, Error> {
        let response = self
            .client
            .post(self.base_url.join(&format!("/files/{guid}/tags"))?)
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON))
            .header(header::AUTHORIZATION, self.get_auth_header())
            .json(&tags)
            .send()
            .await?;

        let result = response.json().await?;

        Ok(result)
    }

    async fn delete_file_tags(&self, guid: &str, tags: Vec<String>) -> Result<(), Error> {
        let response = self
            .client
            .delete(self.base_url.join(&format!("/files/{guid}/tags"))?)
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON))
            .header(header::AUTHORIZATION, self.get_auth_header())
            .json(&tags)
            .send()
            .await?;

        let _result: Vec<String> = response.json().await?;

        Ok(())
    }

    async fn xdg_open_file(&self, file: File) -> Result<ExitStatus, Error> {
        let tempdir = env::temp_dir().join("read-flow");

        if !tempdir.exists() {
            tokio::fs::create_dir(&tempdir).await?;
        }

        let file_path = PathBuf::from(file.path);
        let mut filename = tempdir.join(PathBuf::from(file_path.file_name().unwrap()));

        if !filename.exists() || fingerprint_of(&filename).await? != file.fingerprint {
            filename = self.download_file(&file.guid, &filename).await?;
        }

        // Note that xdg-open will exit while the application is still running, so
        // we cannot delete `filename` after this line.
        let status = Command::new("xdg-open").arg(&filename).status().await?;

        Ok(status)
    }

    async fn delete_file(&self, file: File) -> Result<(), Error> {
        let response = self
            .client
            .delete(self.base_url.join(&format!("files/{}", file.guid))?)
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON))
            .header(header::AUTHORIZATION, self.get_auth_header())
            .send()
            .await?;

        // Check if the request was successful
        response.error_for_status_ref()?;

        Ok(())
    }

    async fn import_file(&self, path: &Path) -> Result<File, Error> {
        self.upload_file(path).await
    }

    async fn get_reading_progress(
        &self,
        fingerprint: &str,
    ) -> Result<Option<ReadingProgress>, Error> {
        let response = self
            .client
            .get(
                self.base_url
                    .join(&format!("reading-progress/{fingerprint}"))?,
            )
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON))
            .header(header::AUTHORIZATION, self.get_auth_header())
            .send()
            .await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        let result = response.json().await?;
        Ok(Some(result))
    }

    async fn upsert_reading_progress(&self, progress: ReadingProgress) -> Result<(), Error> {
        let response = self
            .client
            .put(self.base_url.join("reading-progress")?)
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON))
            .header(header::AUTHORIZATION, self.get_auth_header())
            .json(&progress)
            .send()
            .await?;

        response.error_for_status_ref()?;

        Ok(())
    }
}

impl FilesClient {
    pub async fn get_documents(&self) -> Result<Vec<ApiDocument>, Error> {
        self.get_json("documents").await
    }

    pub async fn get_document(&self, guid: &str) -> Result<Option<ApiDocument>, Error> {
        let response = self
            .client
            .get(self.base_url.join(&format!("documents/{guid}"))?)
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON))
            .header(header::AUTHORIZATION, self.get_auth_header())
            .send()
            .await?;
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
        let response = self
            .client
            .put(self.base_url.join(&format!("documents/{guid}/metadata"))?)
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON))
            .header(header::AUTHORIZATION, self.get_auth_header())
            .json(&meta)
            .send()
            .await?;
        response.error_for_status_ref()?;
        Ok(response.json().await?)
    }

    pub async fn get_document_extracted_metadata(
        &self,
        guid: &str,
    ) -> Result<Option<ContentMetadata>, Error> {
        let response = self
            .client
            .get(
                self.base_url
                    .join(&format!("documents/{guid}/extracted-metadata"))?,
            )
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON))
            .header(header::AUTHORIZATION, self.get_auth_header())
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        Ok(response.json().await?)
    }
}

async fn fingerprint_of(filename: &Path) -> Result<String, Error> {
    let output = Command::new("sha256sum").arg(filename).output().await?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let fingerprint = stdout.split(' ').next().expect("expected fingerprint");
    Ok(fingerprint.to_string())
}
