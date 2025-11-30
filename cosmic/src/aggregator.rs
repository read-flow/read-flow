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

use crate::client::Client;
use crate::client::ClientSelector;
use crate::client::FilesClientError;

#[derive(Clone)]
pub struct Aggregator {
    clients: HashMap<ClientSelector, Client>,
}

unsafe impl Send for Aggregator {}
unsafe impl Sync for Aggregator {}
unsafe impl Send for Documents {}
unsafe impl Sync for Documents {}

impl Aggregator {
    pub fn new(clients: Vec<Client>) -> Self {
        Self {
            clients: clients
                .into_iter()
                .map(|client| (client.selector(), client))
                .collect(),
        }
    }

    pub fn add(&mut self, client: Client) -> Option<Client> {
        self.clients.insert(client.selector(), client)
    }

    pub fn client_for(&self, selector: &ClientSelector) -> Option<&Client> {
        self.clients.get(selector)
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
        for result in results {
            let (selector, files) = result?;
            for file in files {
                documents.push((selector.clone(), file).into());
            }
        }

        Ok(documents)
    }

    pub async fn get_file_tags(&self) -> Result<Vec<String>, FilesClientError> {
        let tags = self
            .aggregate()
            .await?
            .into_iter()
            .flat_map(|document| document.metadata.tags)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        Ok(tags)
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
        results.into_iter().collect::<Result<Vec<_>, _>>()?;

        Ok(())
    }

    pub async fn xdg_open_file(&self, document: Document) -> Result<ExitStatus, FilesClientError> {
        let source = document.local_or_any_source().clone();
        let client = self.client_for(&source.client).unwrap(); // TODO: error
        let file = File::from(SingleDocumentSource(source, document.metadata));

        client.xdg_open_file(file).await
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
        results.into_iter().collect::<Result<Vec<_>, _>>()?;

        Ok(())
    }

    pub async fn reload_document(&self, document: Document) -> Result<Document, FilesClientError> {
        let number_of_sources = document.sources.len();
        let files: Vec<Result<(ClientSelector, Option<File>), FilesClientError>> =
            stream::iter(self.iter_document(document))
                .map(|(client, file)| async move {
                    Ok((client.selector(), client.get_file(file.id).await?))
                })
                .buffer_unordered(number_of_sources)
                .collect()
                .await;

        let documents = files
            .into_iter()
            .filter_map(|result| match result {
                Ok((selector, Some(file))) => Some(Ok((selector, file).into())),
                Ok((_, None)) => None,
                Err(err) => Some(Err(err)),
            })
            .collect::<Result<Vec<Document>, FilesClientError>>()?;

        let mut retval = Documents::default();
        for document in documents {
            retval.push(document);
        }

        Ok(retval.into_single_document())
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
        for result in results {
            retval.extend(result?);
        }

        // TODO: sort alphabetically?
        Ok(retval.into_iter().collect())
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

    pub fn into_single_document(self) -> Document {
        assert_eq!(self.0.len(), 1);
        self.into_iter().next().unwrap()
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
