use std::{
    convert::Infallible,
    env,
    ffi::OsStr,
    io,
    path::{Path, PathBuf},
    process::ExitStatus,
    sync::Arc,
};

use futures::StreamExt;
use reqwest::{Client, Url, header};
use tokio::{fs, process::Command};

use crate::{
    api::{File, FileDataSource, Status},
    extension_of, to_unique_file,
};

#[derive(Clone)]
pub struct FilesClient {
    pub base_url: Url,
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
    pub fn new<U: Into<Url>>(base_url: U) -> Result<Self, Error> {
        let result = Self {
            base_url: base_url.into(),
            client: Client::new(),
        };
        Ok(result)
    }

    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    async fn get_json<T>(&self, relative_url: &str) -> Result<T, Error>
    where
        T: for<'a> serde::Deserialize<'a>,
    {
        let response = self
            .client
            .get(self.base_url.join(relative_url)?)
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON))
            .header(header::AUTHORIZATION, "bearer secret")
            .send()
            .await?;

        let result = response.json().await?;

        Ok(result)
    }

    fn get_target_file(filename: &str) -> Result<PathBuf, Error> {
        let mut file_path: PathBuf = filename.parse().map_err(Error::Unexpected)?;

        let orig_extension =
            extension_of(filename).ok_or(Error::MissingExtension(filename.to_string()))?;
        to_unique_file(&mut file_path, orig_extension);
        Ok(file_path)
    }

    pub async fn download_file(&self, id: i32, filename: &Path) -> Result<PathBuf, Error> {
        let response = self
            .client
            .get(self.base_url.join(&format!(
                "files/{id}/download-as/{}",
                filename.file_name().and_then(OsStr::to_str).unwrap()
            ))?)
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON))
            .header(header::AUTHORIZATION, "bearer secret")
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
            .header(header::AUTHORIZATION, "bearer secret")
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
        self.get_json("status").await
    }

    async fn get_files(&self) -> Result<Vec<File>, Error> {
        self.get_json("files").await
    }

    async fn get_files_tags(&self) -> Result<Vec<String>, Error> {
        self.get_json("files/tags").await
    }

    async fn get_file(&self, id: i32) -> Result<Option<File>, Error> {
        self.get_json(&format!("files/{id}")).await
    }

    async fn update_file(&self, file: File) -> Result<(), Error> {
        let response = self
            .client
            .put(self.base_url.join("/files")?)
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON))
            .header(header::AUTHORIZATION, "bearer secret")
            .json(&file)
            .send()
            .await?;

        response.error_for_status_ref()?;

        Ok(())
    }

    async fn get_file_tags(&self, id: i32) -> Result<Vec<String>, Error> {
        self.get_json(&format!("files/{id}/tags")).await
    }

    async fn add_file_tags(&self, id: i32, tags: Vec<String>) -> Result<Vec<String>, Error> {
        let response = self
            .client
            .post(self.base_url.join(&format!("/files/{id}/tags"))?)
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON))
            .header(header::AUTHORIZATION, "bearer secret")
            .json(&tags)
            .send()
            .await?;

        let result = response.json().await?;

        Ok(result)
    }

    async fn delete_file_tags(&self, id: i32, tags: Vec<String>) -> Result<(), Error> {
        let response = self
            .client
            .delete(self.base_url.join(&format!("/files/{id}/tags"))?)
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON))
            .header(header::AUTHORIZATION, "bearer secret")
            .json(&tags)
            .send()
            .await?;

        let _result: Vec<String> = response.json().await?;

        Ok(())
    }

    async fn xdg_open_file(&self, file: File) -> Result<ExitStatus, Error> {
        let tempdir = env::temp_dir().join("archive-organizer");

        if !tempdir.exists() {
            tokio::fs::create_dir(&tempdir).await?;
        }

        let file_path = PathBuf::from(file.path);
        let mut filename = tempdir.join(PathBuf::from(file_path.file_name().unwrap()));

        if !filename.exists() || fingerprint_of(&filename).await? != file.fingerprint {
            filename = self.download_file(file.id, &filename).await?;
        }

        // Note that xdg-open will exit while the application is still running, so
        // we cannot delete `filename` after this line.
        let status = Command::new("xdg-open").arg(&filename).status().await?;

        Ok(status)
    }

    async fn delete_file(&self, file: File) -> Result<(), Error> {
        // Send a DELETE request to the server
        let response = self
            .client
            .delete(self.base_url.join(&format!("files/{}", file.id))?)
            .header(header::ACCEPT, format!("{}", mime::APPLICATION_JSON))
            .header(header::AUTHORIZATION, "bearer secret")
            .send()
            .await?;

        // Check if the request was successful
        response.error_for_status_ref()?;

        Ok(())
    }
}

async fn fingerprint_of(filename: &Path) -> Result<String, Error> {
    let output = Command::new("sha256sum").arg(filename).output().await?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let fingerprint = stdout.split(' ').next().expect("expected fingerprint");
    Ok(fingerprint.to_string())
}
