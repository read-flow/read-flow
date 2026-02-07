use std::fmt;
use std::path::Path;
use std::process::ExitStatus;
use std::sync::Arc;

use archive_organizer::api::File;
use archive_organizer::api::FileDataSource;
use archive_organizer::api::Status;
use archive_organizer::client;
use archive_organizer::client::FilesClient;
use archive_organizer::db::dao;
use url::Url;

use crate::ApplicationModule;

#[derive(Debug, thiserror::Error)]
pub enum FilesClientError {
    #[error("local files error: {0}")]
    Local(dao::Error),
    #[error("remote files error: {0}")]
    Remote(client::Error),
    #[error("no sources available for document")]
    NoSourcesAvailable,
}

impl From<dao::Error> for FilesClientError {
    fn from(value: dao::Error) -> Self {
        FilesClientError::Local(value)
    }
}

impl From<client::Error> for FilesClientError {
    fn from(value: client::Error) -> Self {
        FilesClientError::Remote(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ClientSelector {
    Local,
    Remote(Url),
}

impl ClientSelector {
    pub fn is_local(&self) -> bool {
        matches!(self, ClientSelector::Local)
    }
}

// TODO: i18n
impl fmt::Display for ClientSelector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClientSelector::Local => write!(f, "Local"),
            ClientSelector::Remote(url) => {
                write!(f, "{}", url.host_str().unwrap_or("Remote"))
            }
        }
    }
}

#[derive(Clone)]
pub enum Client {
    Local(Arc<ApplicationModule>),
    Remote(FilesClient),
}

impl Client {
    pub fn selector(&self) -> ClientSelector {
        match self {
            Client::Local(_) => ClientSelector::Local,
            Client::Remote(client) => ClientSelector::Remote(client.base_url.clone()),
        }
    }
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Client::Local(_) => write!(f, "Local database client"),
            Client::Remote(remote) => {
                write!(f, "Remote HTTP client: {}", remote.base_url())
            }
        }
    }
}

impl From<Arc<ApplicationModule>> for Client {
    fn from(value: Arc<ApplicationModule>) -> Self {
        Client::Local(value)
    }
}

impl From<FilesClient> for Client {
    fn from(value: FilesClient) -> Self {
        Client::Remote(value)
    }
}

macro_rules! delegate {
    ( $e:expr, $f:ident ) => {
        {
	    match $e {
		Client::Local(client) => Ok(client.db_client().$f().await?),
		Client::Remote(client) => Ok(client.$f().await?),
	    }
        }
    };
    ( $e:expr, $f:ident, $( $x:expr ),* ) => {
        {
	    match $e {
		Client::Local(client) => Ok(client.db_client().$f($($x),*).await?),
		Client::Remote(client) => Ok(client.$f($($x),*).await?),
	    }
        }
    };
}

#[async_trait::async_trait]
impl FileDataSource for Client {
    type Error = FilesClientError;

    fn display_name(&self) -> String {
        match self {
            Client::Local(client) => client.db_client().display_name(),
            Client::Remote(client) => client.display_name(),
        }
    }

    async fn status(&self) -> Result<Status, Self::Error> {
        delegate!(self, status)
    }

    async fn get_files(&self) -> Result<Vec<File>, Self::Error> {
        delegate!(self, get_files)
    }

    async fn get_files_tags(&self) -> Result<Vec<String>, Self::Error> {
        delegate!(self, get_files_tags)
    }

    async fn get_file(&self, id: i32) -> Result<Option<File>, Self::Error> {
        delegate!(self, get_file, id)
    }

    async fn get_file_tags(&self, id: i32) -> Result<Vec<String>, Self::Error> {
        delegate!(self, get_file_tags, id)
    }

    async fn add_file_tags(&self, id: i32, tags: Vec<String>) -> Result<Vec<String>, Self::Error> {
        delegate!(self, add_file_tags, id, tags)
    }

    async fn delete_file_tags(&self, id: i32, tags: Vec<String>) -> Result<(), Self::Error> {
        delegate!(self, delete_file_tags, id, tags)
    }

    async fn update_file(&self, file: File) -> Result<(), Self::Error> {
        delegate!(self, update_file, file)
    }

    async fn xdg_open_file(&self, file: File) -> Result<ExitStatus, Self::Error> {
        delegate!(self, xdg_open_file, file)
    }

    async fn delete_file(&self, file: File) -> Result<(), Self::Error> {
        delegate!(self, delete_file, file)
    }

    async fn import_file(&self, path: &Path) -> Result<File, Self::Error> {
        delegate!(self, import_file, path)
    }
}
