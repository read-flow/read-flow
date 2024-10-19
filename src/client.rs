use std::{convert::Infallible, io, path::PathBuf};

use futures::StreamExt;
use reqwest::{header, multipart, Body, Client, Url};
use tokio::fs;
use tokio_util::codec::{BytesCodec, FramedRead};

use crate::{extension_of, serve::models::File, to_unique_file};

pub struct FilesClient {
    base_url: Url,
    client: Client,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid URL: {0}")]
    Url(#[from] url::ParseError),
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("file system error: {0}")]
    IO(#[from] io::Error),
    #[error("file name is missing extension: {0}")]
    MissingExtension(String),
    #[error("the source file doesn't exist: {0}")]
    SourceDoesntExist(PathBuf),
    #[error("programmer error")]
    Unexpected(Infallible),
    #[error("invalid path: {0}")]
    InvalidFile(PathBuf),
}

impl FilesClient {
    pub fn new<U: Into<Url>>(base_url: U) -> Result<Self, Error> {
        let result = Self {
            base_url: base_url.into(),
            client: Client::new(),
        };
        Ok(result)
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

    pub async fn get_files(&self) -> Result<Vec<File>, Error> {
        self.get_json("files").await
    }

    pub async fn get_files_tags(&self) -> Result<Vec<String>, Error> {
        self.get_json("files/tags").await
    }

    pub async fn get_file(&self, id: i32) -> Result<File, Error> {
        self.get_json(&format!("files/{id}")).await
    }

    pub async fn get_file_tags(&self, id: i32) -> Result<Vec<String>, Error> {
        self.get_json(&format!("files/{id}/tags")).await
    }

    fn get_target_file(filename: &str) -> Result<PathBuf, Error> {
        let mut file_path: PathBuf = filename.parse().map_err(Error::Unexpected)?;

        let orig_extension =
            extension_of(filename).ok_or(Error::MissingExtension(filename.to_string()))?;
        to_unique_file(&mut file_path, orig_extension);
        Ok(file_path)
    }

    pub async fn download_file(&self, id: i32, filename: &str) -> Result<(), Error> {
        let response = self
            .client
            .get(
                self.base_url
                    .join(&format!("files/{id}/download-as/{filename}"))?,
            )
            .header(header::AUTHORIZATION, "bearer secret")
            .send()
            .await?;

        let mut bytes = response.bytes_stream();

        let target_filename = Self::get_target_file(filename)?;
        let mut target_file = fs::File::create(&target_filename).await?;

        while let Some(item) = bytes.next().await {
            tokio::io::copy(&mut item?.as_ref(), &mut target_file).await?;
        }

        Ok(())
    }

    pub async fn upload_file(&self, filename: PathBuf) -> Result<File, Error> {
        if !filename.exists() {
            return Err(Error::SourceDoesntExist(filename));
        }

        let file = fs::File::open(&filename).await?;
        let stream = FramedRead::new(file, BytesCodec::new());
        let part = multipart::Part::stream(Body::wrap_stream(stream));

        let filename_clone = filename.clone();

        let form = reqwest::multipart::Form::new()
            .text(
                "filename",
                filename
                    .into_os_string()
                    .into_string()
                    .map_err(|_| Error::InvalidFile(filename_clone))?,
            )
            .part("file", part);

        let response = self
            .client
            .post(self.base_url.join("/files")?)
            .header(header::AUTHORIZATION, "bearer secret")
            .multipart(form)
            .send()
            .await?;

        let result = response.json().await?;

        Ok(result)
    }
}
