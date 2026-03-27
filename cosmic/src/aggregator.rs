use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::hash_map::Entry;
use std::collections::hash_map::IntoValues;
use std::fmt;
use std::iter::repeat_n;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::str::FromStr;
use std::sync::Arc;

use futures_util::stream;
use futures_util::stream::StreamExt;
use provider::r#async::Provider;
use read_flow_core::api::File;
use read_flow_core::api::FileDataSource;
use read_flow_core::api::ReadingProgress;
use read_flow_core::api::ReadingStatus;

use crate::ApplicationModule;
use crate::client::Client;
use crate::client::ClientSelector;
use crate::client::FilesClientError;

pub struct Aggregator {
    clients: HashMap<ClientSelector, Client>,
    application_module: Arc<ApplicationModule>,
}

impl Aggregator {
    pub fn new(clients: Vec<Client>, application_module: Arc<ApplicationModule>) -> Self {
        Self {
            clients: clients
                .into_iter()
                .map(|client| (client.selector(), client))
                .collect(),
            application_module,
        }
    }

    pub async fn _add_available(&mut self, clients: Vec<Client>) {
        stream::iter(clients)
            .fold(self, |acc, client| async move {
                match client.status().await {
                    Ok(_) => {
                        acc.add(client);
                    }
                    Err(error) => {
                        tracing::error!("could not get status for {}: {error}", client.selector())
                    }
                }
                acc
            })
            .await;
    }

    pub fn add(&mut self, client: Client) -> Option<Client> {
        self.clients.insert(client.selector(), client)
    }

    pub fn remove(&mut self, selector: &ClientSelector) -> Option<Client> {
        self.clients.remove(selector)
    }

    pub fn client_for(&self, selector: &ClientSelector) -> Option<&Client> {
        self.clients.get(selector)
    }

    pub fn client_selectors(&self) -> Vec<ClientSelector> {
        self.clients.keys().cloned().collect()
    }

    fn filter_out_hidden_files(&self, files: Vec<File>) -> Vec<File> {
        let ui_settings = self.application_module.settings().ui;
        files
            .into_iter()
            .filter(|file| !ui_settings.contains_hidden_tag(&file.tags))
            .collect()
    }

    pub async fn aggregate(&self) -> Result<Documents, FilesClientError> {
        // Clone clients into a Vec to avoid lifetime issues with async closures
        let clients: Vec<(ClientSelector, Client)> = self
            .clients
            .iter()
            .map(|(s, c)| (s.clone(), c.clone()))
            .collect();

        // Create a stream of futures that fetch files from each client in parallel
        let results: Vec<Result<(ClientSelector, Vec<File>), FilesClientError>> =
            stream::iter(clients)
                .map(|(selector, client)| async move {
                    let files = client.get_files().await?;
                    let files = self.filter_out_hidden_files(files);
                    Ok((selector, files))
                })
                .buffer_unordered(self.clients.len())
                .collect()
                .await;

        // Process results and aggregate documents
        let documents = results
            .into_iter()
            .filter_map(move |result| match result {
                Ok(result) => Some(result),
                Err(error) => {
                    tracing::warn!("ignoring error while retrieving files: {error}");
                    None
                }
            })
            .flat_map(|(selector, files)| repeat_n(selector, files.len()).zip(files))
            .fold(Documents::default(), |mut acc, item| {
                acc.push(item.into());
                acc
            });

        Ok(documents)
    }

    fn iter_document(&self, document: Document) -> impl Iterator<Item = (Client, File)> {
        let files: Vec<_> = document.into();

        files
            .into_iter()
            .map(|(s, f)| (self.clients[&s].clone(), f))
    }

    pub async fn update_document(&self, document: Document) -> Result<(), FilesClientError> {
        let number_of_sources = document.sources.len();
        let results: Vec<Result<(), FilesClientError>> = stream::iter(self.iter_document(document))
            .map(|(client, file)| async move { client.update_file(file).await })
            .buffer_unordered(number_of_sources)
            .collect()
            .await;

        // Log all errors
        results
            .into_iter()
            .filter_map(Result::err)
            .for_each(|error| tracing::warn!("ignoring error during `update_file`: {error}"));

        Ok(())
    }

    /// Open a document using xdg-open, trying clients in priority order.
    ///
    /// Tries local client first, then remote clients.
    /// If opening from one client fails, the next client is tried.
    pub async fn xdg_open_file(&self, document: Document) -> Result<ExitStatus, FilesClientError> {
        let sources = document.sources_by_priority();

        let clients = sources.into_iter().filter_map(|source| {
            self.client_for(&source.client)
                .map(|client| (client, source))
        });

        // Try each source in order until one succeeds
        let mut last_error = None;
        for (client, source) in clients {
            let file = File::from(SingleDocumentSource(
                source.clone(),
                document.metadata.clone(),
            ));
            // Try to open file
            match client.xdg_open_file(file).await {
                Ok(status) => return Ok(status),
                Err(e) => {
                    tracing::warn!(
                        "Failed to open file from source `{}`: {e}",
                        client.selector()
                    );
                    last_error = Some(e);
                }
            }
        }

        // All clients failed, or no clients available
        Err(last_error.unwrap_or(FilesClientError::NoSourcesAvailable))
    }

    pub async fn delete_document_tags(
        &self,
        document: Document,
        tags: &[String],
    ) -> Result<(), FilesClientError> {
        let number_of_sources = document.sources.len();
        let results: Vec<Result<(), FilesClientError>> = stream::iter(self.iter_document(document))
            .map(|(client, file)| {
                let tags = tags.to_vec();
                async move { client.delete_file_tags(file.id, tags).await }
            })
            .buffer_unordered(number_of_sources)
            .collect()
            .await;

        // Log all errors
        results
            .into_iter()
            .filter_map(Result::err)
            .for_each(|error| tracing::warn!("ignoring error during `delete_file_tags`: {error}"));

        Ok(())
    }

    pub async fn add_document_tags(
        &self,
        document: Document,
        tags: &[String],
    ) -> Result<Vec<String>, FilesClientError> {
        let number_of_sources = document.sources.len();
        let results: Vec<Result<Vec<String>, FilesClientError>> =
            stream::iter(self.iter_document(document))
                .map(|(client, file)| {
                    let tags = tags.to_vec();
                    async move { client.add_file_tags(file.id, tags).await }
                })
                .buffer_unordered(number_of_sources)
                .collect()
                .await;

        // Process results and aggregate tags
        let retval = results
            .into_iter()
            .filter_map(move |result| match result {
                Ok(result) => Some(result),
                Err(error) => {
                    tracing::warn!("ignoring error during `add_file_tags`: {error}");
                    None
                }
            })
            .fold(HashSet::new(), |mut acc, item| {
                acc.extend(item);
                acc
            });

        // Sort alphabetically for consistent ordering
        let mut tags: Vec<_> = retval.into_iter().collect();
        tags.sort();
        Ok(tags)
    }

    /// Delete a single source of a document.
    ///
    /// Finds the client for the source and calls `delete_file` on it.
    pub async fn delete_document_source(
        &self,
        source: DocumentSource,
        metadata: DocumentMetadata,
    ) -> Result<(), FilesClientError> {
        let client = self
            .client_for(&source.client)
            .ok_or(FilesClientError::NoSourcesAvailable)?;
        let file = File::from(SingleDocumentSource(source, metadata));
        client.delete_file(file).await
    }

    /// Send a document to a client that doesn't have it yet.
    ///
    /// Finds an existing source for the document (preferring local),
    /// downloads the file if needed, then imports it to the target client.
    pub async fn send_document_to_client(
        &self,
        document: &Document,
        target: &ClientSelector,
    ) -> Result<File, FilesClientError> {
        let target_client = self
            .client_for(target)
            .ok_or(FilesClientError::NoSourcesAvailable)?;

        // Find a source to get the file from (prefer local)
        let sources = document.sources_by_priority();

        let local_source = sources.iter().find(|s| s.client.is_local());

        let local_path = if let Some(source) = local_source {
            // File exists locally, use its path directly
            PathBuf::from(&source.path)
        } else {
            // Need to download from a remote source
            let source = sources
                .first()
                .ok_or(FilesClientError::NoSourcesAvailable)?;
            let source_client = self
                .client_for(&source.client)
                .ok_or(FilesClientError::NoSourcesAvailable)?;

            match source_client {
                Client::Remote(files_client) => {
                    let download_folder = self
                        .application_module
                        .settings()
                        .client
                        .download_folder
                        .into_inner();
                    let _ = tokio::fs::create_dir_all(&download_folder).await;
                    let file_path = PathBuf::from(&source.path);
                    let filename = download_folder.join(file_path.file_name().unwrap());
                    files_client
                        .download_file(source.id, &filename)
                        .await
                        .map_err(FilesClientError::Remote)?
                }
                Client::Local(_) => {
                    // This shouldn't happen - we checked for local sources above
                    return Err(FilesClientError::NoSourcesAvailable);
                }
            }
        };

        // Import the file to the target client
        target_client.import_file(&local_path).await
    }

    /// Get reading progress for a document, picking the most recently updated
    /// progress across all sources.
    pub async fn get_reading_progress(
        &self,
        fingerprint: &str,
    ) -> Result<Option<ReadingProgress>, FilesClientError> {
        let clients: Vec<Client> = self.clients.values().cloned().collect();

        let results: Vec<Result<Option<ReadingProgress>, FilesClientError>> = stream::iter(clients)
            .map(|client| {
                let fp = fingerprint.to_string();
                async move { client.get_reading_progress(&fp).await }
            })
            .buffer_unordered(self.clients.len())
            .collect()
            .await;

        let best = results
            .into_iter()
            .filter_map(|result| match result {
                Ok(Some(progress)) => Some(progress),
                Ok(None) => None,
                Err(error) => {
                    tracing::warn!("ignoring error while retrieving reading progress: {error}");
                    None
                }
            })
            .max_by(|a, b| a.last_updated.cmp(&b.last_updated));

        Ok(best)
    }

    /// Write reading progress to all sources in parallel.
    /// Each source applies last-updated-wins independently.
    pub async fn upsert_reading_progress(
        &self,
        progress: ReadingProgress,
    ) -> Result<(), FilesClientError> {
        let clients: Vec<Client> = self.clients.values().cloned().collect();
        let num_clients = clients.len();

        let results: Vec<Result<(), FilesClientError>> = stream::iter(clients)
            .map(|client| {
                let progress = progress.clone();
                async move { client.upsert_reading_progress(progress).await }
            })
            .buffer_unordered(num_clients)
            .collect()
            .await;

        results
            .into_iter()
            .filter_map(Result::err)
            .for_each(|error| {
                tracing::warn!("ignoring error during `upsert_reading_progress`: {error}")
            });

        Ok(())
    }
}

impl Provider<Documents> for Aggregator {
    type Error = FilesClientError;

    async fn provide(&self) -> Result<Documents, Self::Error> {
        self.aggregate().await
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentType {
    Pdf,
    Epub,
    Mobi,
    /// Any file type not natively supported — opened via the external viewer.
    Other,
}

impl DocumentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            DocumentType::Pdf => "pdf",
            DocumentType::Epub => "epub",
            DocumentType::Mobi => "mobi",
            DocumentType::Other => "other",
        }
    }

    // Get appropriate file type icon based on extension
    pub fn get_file_type_icon(&self) -> &'static str {
        match self {
            DocumentType::Pdf => "application-pdf",
            DocumentType::Epub => "application-epub+zip",
            DocumentType::Mobi => "application-x-mobipocket-ebook",
            DocumentType::Other => "text-x-generic",
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("unsupported document type: {0}")]
pub struct UnsupportedDocumentType(String);

impl FromStr for DocumentType {
    type Err = UnsupportedDocumentType;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lowercase = s.to_ascii_lowercase();
        match lowercase.as_str() {
            "pdf" => Ok(Self::Pdf),
            "epub" => Ok(Self::Epub),
            "mobi" => Ok(Self::Mobi),
            _ => Err(UnsupportedDocumentType(lowercase)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DocumentMetadata {
    pub type_: DocumentType,
    pub size: i32,
    pub fingerprint: String,
    pub tags: Vec<String>,
    pub status: ReadingStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DocumentSource {
    pub id: i32,
    pub path: String,
    pub client: ClientSelector,
}

#[derive(Clone)]
pub struct Document {
    pub metadata: DocumentMetadata,
    pub sources: HashSet<DocumentSource>,
}

impl fmt::Debug for Document {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}:{}", self.metadata.type_, self.metadata.fingerprint)
    }
}

impl Document {
    /// Build a minimal `Document` from a local file path for CLI-initiated opening.
    /// The fingerprint is the canonicalized absolute path (guaranteed unique on a local fs).
    /// For unknown extensions the type is `DocumentType::Other`, which routes to the
    /// external viewer.
    pub fn from_local_path(path: &std::path::Path) -> Option<Self> {
        let abs_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let fingerprint = abs_path.to_string_lossy().into_owned();
        let doc_type = abs_path
            .extension()
            .and_then(|e| e.to_str())
            .and_then(|e| e.parse::<DocumentType>().ok())
            .unwrap_or(DocumentType::Other);
        Some(Document {
            metadata: DocumentMetadata {
                type_: doc_type,
                size: 0,
                fingerprint,
                tags: Vec::new(),
                status: ReadingStatus::Unread,
            },
            sources: HashSet::from([DocumentSource {
                id: 0,
                path: abs_path.to_string_lossy().into_owned(),
                client: ClientSelector::Local,
            }]),
        })
    }

    pub fn local_or_any_source(&self) -> &DocumentSource {
        self.sources
            .iter()
            .find(|source| source.client.is_local())
            .or_else(|| self.sources.iter().next())
            .unwrap()
    }

    /// Returns sources in priority order: local sources first, then remote sources.
    pub fn sources_by_priority(&self) -> Vec<&DocumentSource> {
        let (mut local, mut remote) = self
            .sources
            .iter()
            .partition::<Vec<_>, _>(|s| s.client.is_local());

        local.append(&mut remote);
        local
    }

    pub fn get_client_selectors(&self) -> HashSet<ClientSelector> {
        self.sources
            .iter()
            .map(|source| source.client.clone())
            .collect()
    }
}

#[derive(Clone, Default)]
pub struct Documents(HashMap<String, Document>);

impl fmt::Debug for Documents {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let documents_count = self.0.len();
        if documents_count == 1 {
            write!(f, "{documents_count} document")
        } else {
            write!(f, "{documents_count} documents")
        }
    }
}

impl Documents {
    pub fn push(&mut self, document: Document) {
        match self.0.entry(document.metadata.fingerprint.clone()) {
            Entry::Occupied(mut occupied_entry) => {
                occupied_entry.get_mut().sources.extend(document.sources)
            }
            Entry::Vacant(vacant_entry) => {
                vacant_entry.insert(document);
            }
        }
    }

    pub fn into_iter(self) -> IntoValues<String, Document> {
        self.0.into_values()
    }

    pub fn get(&self, fingerprint: &str) -> Option<&Document> {
        self.0.get(fingerprint)
    }
}

impl From<(ClientSelector, File)> for Document {
    fn from((client, file): (ClientSelector, File)) -> Self {
        Document {
            metadata: DocumentMetadata {
                type_: file.type_.parse().unwrap(), // safe because only supported types are stored in the database
                size: file.size,
                fingerprint: file.fingerprint,
                tags: file.tags,
                status: file.status,
            },
            sources: HashSet::from_iter([DocumentSource {
                id: file.id,
                path: file.path,
                client,
            }]),
        }
    }
}

struct SingleDocumentSource(DocumentSource, DocumentMetadata);

impl From<Document> for Vec<(ClientSelector, File)> {
    fn from(source: Document) -> Self {
        let number_of_sources = source.sources.len();
        source
            .sources
            .into_iter()
            .zip(repeat_n(source.metadata, number_of_sources))
            .map(|(source, metadata)| {
                let selector = source.client.clone();
                (selector, SingleDocumentSource(source, metadata).into())
            })
            .collect()
    }
}

impl From<SingleDocumentSource> for File {
    fn from(source: SingleDocumentSource) -> Self {
        let SingleDocumentSource(source, metadata) = source;
        File {
            id: source.id,
            path: source.path,
            type_: metadata.type_.as_str().to_string(),
            size: metadata.size,
            fingerprint: metadata.fingerprint,
            tags: metadata.tags,
            status: metadata.status,
        }
    }
}
