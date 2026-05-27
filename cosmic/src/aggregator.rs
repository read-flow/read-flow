use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::hash_map::IntoValues;
use std::fmt;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::sync::Arc;

use futures_util::stream;
use futures_util::stream::StreamExt;
use provider::r#async::Provider;
use read_flow_core::api::DocumentMeta;
use read_flow_core::api::File;
use read_flow_core::api::FileDataSource;
use read_flow_core::api::ReadingState;
use read_flow_core::api::ReadingStatus;
pub use read_flow_core::scan::DocumentType;

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

    pub async fn aggregate(&self) -> Result<Documents, FilesClientError> {
        let clients: Vec<(ClientSelector, Client)> = self
            .clients
            .iter()
            .map(|(s, c)| (s.clone(), c.clone()))
            .collect();

        let results: Vec<Result<(ClientSelector, Vec<File>), FilesClientError>> =
            stream::iter(clients)
                .map(|(selector, client)| async move {
                    let files = client.get_files().await?;
                    Ok((selector, files))
                })
                .buffer_unordered(self.clients.len())
                .collect()
                .await;

        let mut documents = results
            .into_iter()
            .filter_map(move |result| match result {
                Ok(result) => Some(result),
                Err(error) => {
                    tracing::warn!("ignoring error while retrieving files: {error}");
                    None
                }
            })
            .flat_map(|(selector, files)| files.into_iter().map(move |f| (selector.clone(), f)))
            .fold(Documents::default(), |mut acc, (selector, file)| {
                let guid = file
                    .document_guid
                    .clone()
                    .unwrap_or_else(|| file.fingerprint.clone());
                acc.push(guid, selector, file);
                acc
            });

        // Fetch user-edited metadata from all clients; local client wins on conflict.
        let ordered_clients: Vec<Client> = {
            let (locals, mut remotes): (Vec<Client>, Vec<Client>) = self
                .clients
                .values()
                .cloned()
                .partition(|c| c.selector().is_local());
            remotes.extend(locals);
            remotes
        };
        let client_count = ordered_clients.len();
        let meta_results: Vec<Result<Vec<_>, FilesClientError>> = stream::iter(ordered_clients)
            .map(|client| async move { client.get_documents().await })
            .buffer_unordered(client_count)
            .collect()
            .await;
        let all_meta: HashMap<String, DocumentMeta> = meta_results
            .into_iter()
            .filter_map(|result| {
                if let Err(ref e) = result {
                    tracing::warn!("ignoring error while retrieving document metadata: {e}");
                }
                result.ok()
            })
            .flatten()
            .map(|d| (d.guid, d.metadata))
            .collect();
        for doc in documents.values_mut() {
            if let Some(meta) = all_meta.get(&doc.document_guid) {
                doc.user_meta = meta.clone();
            }
        }

        Ok(documents)
    }

    /// Update user-edited document metadata on all sources that hold the document.
    ///
    /// Documents with a synthetic guid (fingerprint used as guid) first call
    /// `ensure_document_for_file` to create a real `documents` row, then save.
    pub async fn update_document_metadata(
        &self,
        document: &Document,
        meta: DocumentMeta,
    ) -> Result<(), FilesClientError> {
        let is_synthetic = document
            .contents
            .iter()
            .any(|c| c.fingerprint == document.document_guid);

        if !is_synthetic {
            // Fast path: real document record exists; call once per unique client.
            let unique_selectors: HashSet<ClientSelector> = document
                .contents
                .iter()
                .flat_map(|c| c.sources.iter())
                .map(|s| s.client.clone())
                .collect();
            let clients: Vec<Client> = unique_selectors
                .into_iter()
                .filter_map(|sel| self.clients.get(&sel).cloned())
                .collect();
            let source_count = clients.len().max(1);
            let guid = document.document_guid.clone();

            let results: Vec<Result<Option<_>, FilesClientError>> = stream::iter(clients)
                .map(|client| {
                    let guid = guid.clone();
                    let meta = meta.clone();
                    async move { client.update_document_metadata(&guid, meta).await }
                })
                .buffer_unordered(source_count)
                .collect()
                .await;

            results
                .into_iter()
                .filter_map(Result::err)
                .for_each(|e| tracing::warn!("error updating document metadata: {e}"));
        } else {
            // Slow path: synthetic guid — ensure a real document row exists per source.
            let source_client_pairs: Vec<(String, Client)> = document
                .contents
                .iter()
                .flat_map(|c| c.sources.iter())
                .filter_map(|s| {
                    self.clients
                        .get(&s.client)
                        .map(|c| (s.guid.clone(), c.clone()))
                })
                .collect();
            let source_count = source_client_pairs.len().max(1);

            let results: Vec<Result<_, FilesClientError>> = stream::iter(source_client_pairs)
                .map(|(file_guid, client)| {
                    let meta = meta.clone();
                    async move {
                        let api_doc = client.ensure_document_for_file(&file_guid).await?;
                        client.update_document_metadata(&api_doc.guid, meta).await
                    }
                })
                .buffer_unordered(source_count)
                .collect()
                .await;

            results
                .into_iter()
                .filter_map(Result::err)
                .for_each(|e| tracing::warn!("error ensuring/updating document metadata: {e}"));
        }

        Ok(())
    }

    /// Merge `loser` documents into `winner`, re-assigning all their sources and metadata.
    pub async fn merge_documents(
        &self,
        winner: &Document,
        losers: &[Document],
    ) -> Result<(), FilesClientError> {
        let local_client = self
            .clients
            .get(&ClientSelector::Local)
            .ok_or(FilesClientError::NoSourcesAvailable)?;

        let winner_guid = if winner
            .contents
            .iter()
            .any(|c| c.fingerprint == winner.document_guid)
        {
            let file_guid = winner
                .contents
                .iter()
                .flat_map(|c| c.sources.iter())
                .next()
                .ok_or(FilesClientError::NoSourcesAvailable)?
                .guid
                .clone();
            local_client
                .ensure_document_for_file(&file_guid)
                .await?
                .guid
        } else {
            winner.document_guid.clone()
        };

        let mut loser_guids = Vec::with_capacity(losers.len());
        for loser in losers {
            let guid = if loser
                .contents
                .iter()
                .any(|c| c.fingerprint == loser.document_guid)
            {
                let file_guid = loser
                    .contents
                    .iter()
                    .flat_map(|c| c.sources.iter())
                    .next()
                    .ok_or(FilesClientError::NoSourcesAvailable)?
                    .guid
                    .clone();
                local_client
                    .ensure_document_for_file(&file_guid)
                    .await?
                    .guid
            } else {
                loser.document_guid.clone()
            };
            loser_guids.push(guid);
        }

        local_client
            .merge_documents(&winner_guid, &loser_guids)
            .await?;

        for (selector, client) in &self.clients {
            if selector.is_local() {
                continue;
            }
            if let Err(e) = client.merge_documents(&winner_guid, &loser_guids).await {
                tracing::warn!("error merging documents on {selector}: {e}");
            }
        }

        Ok(())
    }

    fn iter_document(&self, document: Document) -> impl Iterator<Item = (Client, File)> {
        let files: Vec<(ClientSelector, File)> = document.into();
        files.into_iter().filter_map(|(s, f)| {
            let client = self.clients.get(&s).cloned();
            if client.is_none() {
                tracing::warn!("no client for selector {s:?}, skipping file {}", f.guid);
            }
            client.map(|c| (c, f))
        })
    }

    pub async fn update_document(&self, document: Document) -> Result<(), FilesClientError> {
        let number_of_sources: usize = document.contents.iter().map(|c| c.sources.len()).sum();
        let results: Vec<Result<(), FilesClientError>> = stream::iter(self.iter_document(document))
            .map(|(client, file)| async move { client.update_file(file).await })
            .buffer_unordered(number_of_sources.max(1))
            .collect()
            .await;

        results
            .into_iter()
            .filter_map(Result::err)
            .for_each(|error| tracing::warn!("ignoring error during `update_file`: {error}"));

        Ok(())
    }

    /// Open a document using xdg-open, trying clients in priority order.
    pub async fn xdg_open_file(&self, document: Document) -> Result<ExitStatus, FilesClientError> {
        let (mut local, mut remote): (Vec<_>, Vec<_>) = document
            .contents
            .into_iter()
            .flat_map(|content| {
                let DocumentContent {
                    fingerprint,
                    type_,
                    size,
                    tags,
                    status,
                    sources,
                } = content;
                sources.into_iter().map(move |source| {
                    let is_local = source.client.is_local();
                    let content = DocumentContent {
                        fingerprint: fingerprint.clone(),
                        type_,
                        size,
                        tags: tags.clone(),
                        status,
                        sources: vec![],
                    };
                    (content, source, is_local)
                })
            })
            .partition(|(_, _, is_local)| *is_local);
        local.append(&mut remote);

        let mut last_error = None;
        for (content, source, _) in local {
            let Some(client) = self.client_for(&source.client) else {
                continue;
            };
            let file = content_source_to_file(content, source);
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

        Err(last_error.unwrap_or(FilesClientError::NoSourcesAvailable))
    }

    pub async fn delete_document_tags(
        &self,
        document: Document,
        tags: &[String],
    ) -> Result<(), FilesClientError> {
        let number_of_sources: usize = document.contents.iter().map(|c| c.sources.len()).sum();
        let results: Vec<Result<(), FilesClientError>> = stream::iter(self.iter_document(document))
            .map(|(client, file)| {
                let tags = tags.to_vec();
                async move { client.delete_file_tags(&file.guid, tags).await }
            })
            .buffer_unordered(number_of_sources.max(1))
            .collect()
            .await;

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
        let number_of_sources: usize = document.contents.iter().map(|c| c.sources.len()).sum();
        let results: Vec<Result<Vec<String>, FilesClientError>> =
            stream::iter(self.iter_document(document))
                .map(|(client, file)| {
                    let tags = tags.to_vec();
                    async move { client.add_file_tags(&file.guid, tags).await }
                })
                .buffer_unordered(number_of_sources.max(1))
                .collect()
                .await;

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

        let mut tags: Vec<_> = retval.into_iter().collect();
        tags.sort();
        Ok(tags)
    }

    /// Delete a single source of a document.
    pub async fn delete_document_source(
        &self,
        source: DocumentSource,
        content: DocumentContent,
    ) -> Result<(), FilesClientError> {
        let client = self
            .client_for(&source.client)
            .ok_or(FilesClientError::NoSourcesAvailable)?;
        let file = content_source_to_file(content, source);
        client.delete_file(file).await
    }

    /// Send a document to a client that doesn't have it yet.
    pub async fn send_document_to_client(
        &self,
        document: &Document,
        target: &ClientSelector,
    ) -> Result<File, FilesClientError> {
        let target_client = self
            .client_for(target)
            .ok_or(FilesClientError::NoSourcesAvailable)?;

        let sources = document.sources_by_priority();

        let local_source = sources.iter().find(|(_, s)| s.client.is_local());

        let local_path = if let Some((_, source)) = local_source {
            PathBuf::from(&source.path)
        } else {
            let (_, source) = sources
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
                        .await
                        .client
                        .download_folder
                        .into_inner();
                    let _ = tokio::fs::create_dir_all(&download_folder).await;
                    let file_path = PathBuf::from(&source.path);
                    let filename = download_folder.join(file_path.file_name().unwrap());
                    files_client
                        .download_file(&source.guid, &filename)
                        .await
                        .map_err(FilesClientError::Remote)?
                }
                Client::Local(_) => {
                    return Err(FilesClientError::NoSourcesAvailable);
                }
            }
        };

        target_client.import_file(&local_path).await
    }

    /// Get reading state for a document, picking the most recently updated
    /// state across all sources.
    pub async fn get_reading_state(
        &self,
        fingerprint: &str,
    ) -> Result<Option<ReadingState>, FilesClientError> {
        let clients: Vec<Client> = self.clients.values().cloned().collect();

        let results: Vec<Result<Option<ReadingState>, FilesClientError>> = stream::iter(clients)
            .map(|client| {
                let fp = fingerprint.to_string();
                async move { client.get_reading_state(&fp).await }
            })
            .buffer_unordered(self.clients.len())
            .collect()
            .await;

        let best = results
            .into_iter()
            .filter_map(|result| match result {
                Ok(Some(state)) => Some(state),
                Ok(None) => None,
                Err(error) => {
                    tracing::warn!("ignoring error while retrieving reading state: {error}");
                    None
                }
            })
            .max_by(|a, b| a.last_updated.cmp(&b.last_updated));

        Ok(best)
    }

    /// Write reading state to all sources in parallel. Returns the resulting
    /// state from the first successful source (which may have auto-transitioned status).
    pub async fn upsert_reading_state(
        &self,
        state: ReadingState,
    ) -> Result<ReadingState, FilesClientError> {
        let clients: Vec<Client> = self.clients.values().cloned().collect();
        let num_clients = clients.len();

        let mut results: Vec<Result<ReadingState, FilesClientError>> = stream::iter(clients)
            .map(|client| {
                let state = state.clone();
                async move { client.upsert_reading_state(state).await }
            })
            .buffer_unordered(num_clients)
            .collect()
            .await;

        let first_ok = results.iter().position(|r| r.is_ok());
        results
            .iter()
            .filter_map(|r| r.as_ref().err())
            .for_each(|error| {
                tracing::warn!("ignoring error during `upsert_reading_state`: {error}")
            });

        match first_ok {
            Some(i) => Ok(results.remove(i).unwrap()),
            None => Err(results.remove(0).unwrap_err()),
        }
    }

    /// Manually override reading status on all sources in parallel.
    pub async fn update_reading_status(
        &self,
        fingerprint: &str,
        status: ReadingStatus,
    ) -> Result<(), FilesClientError> {
        let clients: Vec<Client> = self.clients.values().cloned().collect();
        let num_clients = clients.len();

        let results: Vec<Result<(), FilesClientError>> = stream::iter(clients)
            .map(|client| {
                let fp = fingerprint.to_string();
                async move { client.update_reading_status(&fp, status).await }
            })
            .buffer_unordered(num_clients)
            .collect()
            .await;

        results
            .into_iter()
            .filter_map(Result::err)
            .for_each(|error| {
                tracing::warn!("ignoring error during `update_reading_status`: {error}")
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

pub use read_flow_core::api::DocumentMeta as UserMeta;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentSource {
    pub guid: String,
    pub path: String,
    pub client: ClientSelector,
    pub size: i32,
}

#[derive(Debug, Clone)]
pub struct DocumentContent {
    pub fingerprint: String,
    pub type_: DocumentType,
    pub size: i32,
    pub tags: Vec<String>,
    pub status: ReadingStatus,
    pub sources: Vec<DocumentSource>,
}

#[derive(Clone)]
pub struct Document {
    /// Primary identity — corresponds to a `documents.guid` row (or a synthetic
    /// fingerprint-based id for files that have no document row yet).
    pub document_guid: String,
    pub user_meta: UserMeta,
    pub contents: Vec<DocumentContent>,
}

impl fmt::Debug for Document {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let type_str = self
            .contents
            .first()
            .map(|c| c.type_.as_str())
            .unwrap_or("?");
        write!(f, "{type_str}:{}", self.document_guid)
    }
}

impl Document {
    /// Build a minimal `Document` from a local file path for CLI-initiated opening.
    pub fn from_local_path(path: &std::path::Path) -> Option<Self> {
        let abs_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let fingerprint = abs_path.to_string_lossy().into_owned();
        let doc_type = abs_path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.parse::<DocumentType>().unwrap())
            .unwrap_or(DocumentType::Other);
        Some(Document {
            document_guid: fingerprint.clone(), // synthetic
            user_meta: UserMeta::default(),
            contents: vec![DocumentContent {
                fingerprint: fingerprint.clone(),
                type_: doc_type,
                size: 0,
                tags: Vec::new(),
                status: ReadingStatus::Unread,
                sources: vec![DocumentSource {
                    guid: String::new(),
                    path: abs_path.to_string_lossy().into_owned(),
                    client: ClientSelector::Local,
                    size: 0,
                }],
            }],
        })
    }

    pub fn local_or_any_source(&self) -> Option<(&DocumentContent, &DocumentSource)> {
        for content in &self.contents {
            for source in &content.sources {
                if source.client.is_local() {
                    return Some((content, source));
                }
            }
        }
        let content = self.contents.first()?;
        Some((content, content.sources.first()?))
    }

    /// Returns (content, source) pairs sorted local-first, then remote.
    pub fn sources_by_priority(&self) -> Vec<(&DocumentContent, &DocumentSource)> {
        let mut local = Vec::new();
        let mut remote = Vec::new();
        for content in &self.contents {
            for source in &content.sources {
                if source.client.is_local() {
                    local.push((content, source));
                } else {
                    remote.push((content, source));
                }
            }
        }
        local.extend(remote);
        local
    }

    pub fn get_client_selectors(&self) -> HashSet<ClientSelector> {
        self.contents
            .iter()
            .flat_map(|c| c.sources.iter())
            .map(|s| s.client.clone())
            .collect()
    }

    pub fn file_types(&self) -> Vec<DocumentType> {
        let mut types: Vec<DocumentType> = self
            .contents
            .iter()
            .map(|c| c.type_)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        types.sort();
        types
    }

    /// Return a copy of this document restricted to the single source with the given `guid`.
    pub fn with_source_guid(&self, guid: &str) -> Option<Document> {
        for content in &self.contents {
            if let Some(source) = content.sources.iter().find(|s| s.guid == guid) {
                return Some(Document {
                    document_guid: self.document_guid.clone(),
                    user_meta: self.user_meta.clone(),
                    contents: vec![DocumentContent {
                        fingerprint: content.fingerprint.clone(),
                        type_: content.type_,
                        size: content.size,
                        tags: content.tags.clone(),
                        status: content.status,
                        sources: vec![source.clone()],
                    }],
                });
            }
        }
        None
    }

    pub fn as_format(&self, type_: DocumentType) -> Option<Document> {
        let matching: Vec<DocumentContent> = self
            .contents
            .iter()
            .filter(|c| c.type_ == type_)
            .cloned()
            .collect();
        if matching.is_empty() {
            return None;
        }
        Some(Document {
            document_guid: self.document_guid.clone(),
            user_meta: self.user_meta.clone(),
            contents: matching,
        })
    }
}

#[derive(Clone, Default)]
pub struct Documents {
    by_document_guid: HashMap<String, Document>,
    /// Secondary index: fingerprint → document_guid, for lookup by content fingerprint.
    fingerprint_to_guid: HashMap<String, String>,
}

impl fmt::Debug for Documents {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let documents_count = self.by_document_guid.len();
        if documents_count == 1 {
            write!(f, "{documents_count} document")
        } else {
            write!(f, "{documents_count} documents")
        }
    }
}

impl Documents {
    pub fn push(&mut self, document_guid: String, selector: ClientSelector, file: File) {
        let type_: DocumentType = file.type_.parse().unwrap();
        let source = DocumentSource {
            guid: file.guid,
            path: file.path,
            client: selector,
            size: file.size,
        };

        if let Some(doc) = self.by_document_guid.get_mut(&document_guid) {
            if let Some(content) = doc
                .contents
                .iter_mut()
                .find(|c| c.fingerprint == file.fingerprint)
            {
                content.sources.push(source);
            } else {
                self.fingerprint_to_guid
                    .insert(file.fingerprint.clone(), document_guid.clone());
                doc.contents.push(DocumentContent {
                    fingerprint: file.fingerprint,
                    type_,
                    size: file.size,
                    tags: file.tags,
                    status: file.status,
                    sources: vec![source],
                });
            }
        } else {
            self.fingerprint_to_guid
                .insert(file.fingerprint.clone(), document_guid.clone());
            let doc = Document {
                document_guid: document_guid.clone(),
                user_meta: UserMeta::default(),
                contents: vec![DocumentContent {
                    fingerprint: file.fingerprint,
                    type_,
                    size: file.size,
                    tags: file.tags,
                    status: file.status,
                    sources: vec![source],
                }],
            };
            self.by_document_guid.insert(document_guid, doc);
        }
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut Document> {
        self.by_document_guid.values_mut()
    }

    pub fn into_iter(self) -> IntoValues<String, Document> {
        self.by_document_guid.into_values()
    }

    pub fn get(&self, document_guid: &str) -> Option<&Document> {
        self.by_document_guid.get(document_guid)
    }
}

impl From<Document> for Vec<(ClientSelector, File)> {
    fn from(document: Document) -> Self {
        let document_guid = document.document_guid.clone();
        document
            .contents
            .into_iter()
            .flat_map(|content| {
                let document_guid = document_guid.clone();
                let type_str = content.type_.as_str().to_string();
                let fingerprint = content.fingerprint;
                let tags = content.tags;
                let status = content.status;
                let size = content.size;
                content.sources.into_iter().map(move |source| {
                    let selector = source.client.clone();
                    let file = File {
                        guid: source.guid,
                        path: source.path,
                        type_: type_str.clone(),
                        size,
                        fingerprint: fingerprint.clone(),
                        tags: tags.clone(),
                        status,
                        document_guid: Some(document_guid.clone()),
                    };
                    (selector, file)
                })
            })
            .collect()
    }
}

fn content_source_to_file(content: DocumentContent, source: DocumentSource) -> File {
    File {
        guid: source.guid,
        path: source.path,
        type_: content.type_.as_str().to_string(),
        size: content.size,
        fingerprint: content.fingerprint,
        tags: content.tags,
        status: content.status,
        document_guid: None,
    }
}
