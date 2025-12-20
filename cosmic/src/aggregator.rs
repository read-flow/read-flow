use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::hash_map::Entry;
use std::collections::hash_map::IntoValues;
use std::iter::repeat_n;
use std::process::ExitStatus;
use std::str::FromStr;

use archive_organizer::api::File;
use archive_organizer::api::FileDataSource;
use archive_organizer::api::ReadingStatus;
use futures_util::stream;
use futures_util::stream::StreamExt;
use provider::r#async::Provider;

use crate::client::Client;
use crate::client::ClientSelector;
use crate::client::FilesClientError;

pub struct Aggregator {
    clients: HashMap<ClientSelector, Client>,
}

impl Aggregator {
    pub fn new(clients: Vec<Client>) -> Self {
        Self {
            clients: clients
                .into_iter()
                .map(|client| (client.selector(), client))
                .collect(),
        }
    }

    pub async fn add_available(&mut self, clients: Vec<Client>) {
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

    pub async fn aggregate(&self) -> Result<Documents, FilesClientError> {
        let mut documents = Documents::default();

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
                    Ok((selector, files))
                })
                .buffer_unordered(self.clients.len())
                .collect()
                .await;

        // Process results and aggregate documents
        for result in results.into_iter().filter(Result::is_ok) {
            let (selector, files) = result?;
            for file in files {
                documents.push((selector.clone(), file).into());
            }
        }

        Ok(documents)
    }

    fn iter_document(&self, document: Document) -> impl Iterator<Item = (Client, File)> {
        let files: HashMap<_, _> = document.into();

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

        // Process results and return first error
        results
            .into_iter()
            .filter(Result::is_ok)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(())
    }

    /// Open a document using xdg-open, trying sources in priority order.
    ///
    /// Tries local sources first, then remote sources.
    /// If opening from one source fails, the next source is tried.
    pub async fn xdg_open_file(&self, document: Document) -> Result<ExitStatus, FilesClientError> {
        let sources = document.sources_by_priority();

        if sources.is_empty() {
            return Err(FilesClientError::NoSourcesAvailable);
        }

        // Create providers for each source
        let providers: Vec<OpenFileProvider> = sources
            .into_iter()
            .filter_map(|source| {
                self.client_for(&source.client).map(|client| {
                    let file = File::from(SingleDocumentSource(
                        source.clone(),
                        document.metadata.clone(),
                    ));
                    OpenFileProvider {
                        client: client.clone(),
                        file,
                    }
                })
            })
            .collect();

        if providers.is_empty() {
            return Err(FilesClientError::NoSourcesAvailable);
        }

        // Try each provider in order until one succeeds
        let mut last_error = None;
        for provider in providers {
            match provider.provide().await {
                Ok(status) => return Ok(status),
                Err(e) => {
                    tracing::debug!("Failed to open file from source: {e}");
                    last_error = Some(e);
                }
            }
        }

        // All providers failed, return the last error
        Err(last_error.unwrap())
    }

    pub async fn delete_document_tags(
        &self,
        document: Document,
        tags: Vec<String>,
    ) -> Result<(), FilesClientError> {
        let number_of_sources = document.sources.len();
        let results: Vec<Result<(), FilesClientError>> = stream::iter(self.iter_document(document))
            .map(|(client, file)| {
                let tags = tags.clone();
                async move { client.delete_file_tags(file.id, tags).await }
            })
            .buffer_unordered(number_of_sources)
            .collect()
            .await;

        // Process results and return first error
        results
            .into_iter()
            .filter(Result::is_ok)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(())
    }

    pub async fn add_document_tags(
        &self,
        document: Document,
        tags: Vec<String>,
    ) -> Result<Vec<String>, FilesClientError> {
        let number_of_sources = document.sources.len();
        let results: Vec<Result<Vec<String>, FilesClientError>> =
            stream::iter(self.iter_document(document))
                .map(|(client, file)| {
                    let tags = tags.clone();
                    async move { client.add_file_tags(file.id, tags).await }
                })
                .buffer_unordered(number_of_sources)
                .collect()
                .await;

        // Process results and aggregate tags
        let mut retval = HashSet::new();
        for result in results.into_iter().filter(Result::is_ok) {
            retval.extend(result?);
        }

        // Sort alphabetically for consistent ordering
        let mut tags: Vec<_> = retval.into_iter().collect();
        tags.sort();
        Ok(tags)
    }
}

impl Provider<Documents> for Aggregator {
    type Error = FilesClientError;

    async fn provide(&self) -> Result<Documents, Self::Error> {
        self.aggregate().await
    }
}

#[derive(Debug, Clone)]
pub enum DocumentType {
    Pdf,
    Epub,
    Mobi,
}

impl DocumentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            DocumentType::Pdf => "pdf",
            DocumentType::Epub => "epub",
            DocumentType::Mobi => "mobi",
        }
    }

    // Get appropriate file type icon based on extension
    pub fn get_file_type_icon(&self) -> &'static str {
        match self {
            DocumentType::Pdf => "application-pdf",
            DocumentType::Epub => "application-epub+zip",
            DocumentType::Mobi => "application-x-mobipocket-ebook",
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

#[derive(Debug, Clone)]
pub struct Document {
    pub metadata: DocumentMetadata,
    pub sources: HashSet<DocumentSource>,
}

impl Document {
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
}

#[derive(Debug, Clone, Default)]
pub struct Documents(HashMap<String, Document>);

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

impl From<Document> for HashMap<ClientSelector, File> {
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

/// A provider that opens a file using xdg-open via a specific client.
struct OpenFileProvider {
    client: Client,
    file: File,
}

impl Provider<ExitStatus> for OpenFileProvider {
    type Error = FilesClientError;

    async fn provide(&self) -> Result<ExitStatus, Self::Error> {
        self.client.xdg_open_file(self.file.clone()).await
    }
}
