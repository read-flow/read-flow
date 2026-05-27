use std::collections::HashSet;
use std::convert::Infallible;
use std::process::ExitStatus;
use std::sync::Arc;

use cosmic::iced::Subscription;
use provider::r#async::Expiring;
use provider::r#async::HasSetExpired;
use provider::r#async::Invalidated;
use provider::r#async::Observable;
use provider::r#async::ObservableCache;
use provider::r#async::Provider;
use read_flow_core::api::ReadingStatus;
use tokio::sync::RwLock;
use tokio::sync::broadcast;

use crate::aggregator::Aggregator;
use crate::aggregator::Document;
use crate::aggregator::DocumentContent;
use crate::aggregator::DocumentSource;
use crate::aggregator::Documents;
use crate::aggregator::UserMeta;
use crate::client::Client;
use crate::client::ClientSelector;
use crate::client::FilesClientError;
use crate::subscription::SubscriberState;

type DocumentsCache =
    ObservableCache<Arc<RwLock<Aggregator>>, fn(Documents) -> Documents, Documents, Documents>;
type TagsCache =
    ObservableCache<Arc<DocumentsCache>, fn(Documents) -> Vec<String>, Documents, Vec<String>>;

/// Extract unique sorted tags from documents.
fn extract_tags(documents: Documents) -> Vec<String> {
    let mut tags = documents
        .into_iter()
        .flat_map(|document| document.contents.into_iter().flat_map(|c| c.tags))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    tags.sort();
    tags
}

pub struct DocumentProvider {
    pub(crate) aggregator: Arc<RwLock<Aggregator>>,
    documents_cache: Arc<DocumentsCache>,
    tags_cache: TagsCache,
}

impl DocumentProvider {
    pub fn new(aggregator: Aggregator) -> Self {
        let aggregator = Arc::new(RwLock::new(aggregator));
        let documents_cache = Arc::new(aggregator.clone().observable_cache());
        let tags_cache = documents_cache
            .clone()
            .observable_cache_with_fn(extract_tags as fn(Documents) -> Vec<String>);

        Self {
            aggregator,
            documents_cache,
            tags_cache,
        }
    }

    pub async fn get_documents(&self) -> Result<Documents, FilesClientError> {
        self.documents_cache.provide().await
    }

    /// Subscribe to cache invalidation notifications.
    ///
    /// Returns a receiver that will receive notifications whenever the cache is invalidated.
    /// This can be used to trigger UI refreshes when data changes.
    pub fn subscribe(&self) -> broadcast::Receiver<Invalidated> {
        self.documents_cache.subscribe()
    }

    /// Create an iced Subscription that emits messages when the cache is invalidated.
    ///
    /// The subscription will emit the result of calling `f` whenever the document cache
    /// is invalidated. This is useful for triggering UI refreshes in cosmic applications.
    ///
    /// # Example
    ///
    /// ```ignore
    /// fn subscription(&self) -> Subscription<Message> {
    ///     self.document_provider.invalidation_subscription(
    ///         "document-invalidation",
    ///         || Message::DocumentsChanged,
    ///     )
    /// }
    /// ```
    pub fn invalidation_subscription<M, F>(&self, f: F) -> Subscription<M>
    where
        M: Send + 'static,
        F: Fn() -> M + Send + Sync + 'static,
        F: Send + Sync + 'static,
    {
        let receiver = self.subscribe();

        Subscription::run_with(SubscriberState::new(receiver, f), SubscriberState::run)
    }

    /// Get all unique tags from all documents.
    ///
    /// Uses a cached mapping provider that derives tags from the document cache.
    /// The cache is automatically invalidated when the document cache expires.
    pub async fn get_all_tags(&self) -> Result<Vec<String>, FilesClientError> {
        self.tags_cache.provide().await
    }

    /// Get a single document by document_guid.
    ///
    /// Uses the cached documents to efficiently look up a single document.
    pub async fn get_document(
        &self,
        document_guid: &str,
    ) -> Result<Option<Document>, FilesClientError> {
        self.get_documents()
            .await
            .map(|docs| docs.get(document_guid).cloned())
    }

    /// Update a document across all sources.
    ///
    /// Automatically invalidates the cache after the update.
    pub async fn update_reading_status(
        &self,
        fingerprint: &str,
        status: ReadingStatus,
    ) -> Result<(), FilesClientError> {
        self.aggregator
            .read()
            .await
            .update_reading_status(fingerprint, status)
            .await
    }

    pub async fn update_document(&self, document: Document) -> Result<(), FilesClientError> {
        let result = self.aggregator.read().await.update_document(document).await;
        self.set_expired().await;
        result
    }

    /// Update user-edited document metadata (title, type, authors, etc.) across all sources.
    ///
    /// Automatically invalidates the cache after the update.
    pub async fn update_document_metadata(
        &self,
        document: &Document,
        meta: UserMeta,
    ) -> Result<(), FilesClientError> {
        let result = self
            .aggregator
            .read()
            .await
            .update_document_metadata(document, meta)
            .await;
        self.set_expired().await;
        result
    }

    /// Add tags to a document across all sources.
    ///
    /// Automatically invalidates the cache after the update.
    pub async fn add_document_tags(
        &self,
        document: Document,
        tags: &[String],
    ) -> Result<Vec<String>, FilesClientError> {
        let result = self
            .aggregator
            .read()
            .await
            .add_document_tags(document, tags)
            .await;
        self.set_expired().await;
        result
    }

    /// Add tags to multiple documents across all sources.
    ///
    /// Automatically invalidates the cache after the update.
    pub async fn batch_add_document_tags(
        &self,
        documents: Vec<Document>,
        tags: &[String],
    ) -> Result<(), FilesClientError> {
        let mut first_error = None;

        for document in documents {
            let result = self
                .aggregator
                .read()
                .await
                .add_document_tags(document, tags)
                .await;

            // Log all errors and remember first error
            if let Err(error) = result {
                tracing::warn!("failed adding tags to document: {error}");
                first_error = first_error.or(Some(error));
            }
        }
        // Expire, even if there were errors
        self.set_expired().await;

        match first_error {
            None => Ok(()),
            Some(error) => Err(error),
        }
    }

    /// Delete tags from a document across all sources.
    ///
    /// Automatically invalidates the cache after the update.
    pub async fn delete_document_tags(
        &self,
        document: Document,
        tags: &[String],
    ) -> Result<(), FilesClientError> {
        let result = self
            .aggregator
            .read()
            .await
            .delete_document_tags(document, tags)
            .await;
        self.set_expired().await;
        result
    }

    /// Delete tags from multiple documents across all sources.
    ///
    /// Automatically invalidates the cache after the update.
    pub async fn batch_delete_document_tags(
        &self,
        documents: Vec<Document>,
        tags: &[String],
    ) -> Result<(), FilesClientError> {
        let mut first_error = None;

        for document in documents {
            let result = self
                .aggregator
                .read()
                .await
                .delete_document_tags(document, tags)
                .await;

            // Log all errors and remember first error
            if let Err(error) = result {
                tracing::warn!("failed deleting tags from document: {error}");
                first_error = first_error.or(Some(error));
            }
        }
        // Expire, even if there were errors
        self.set_expired().await;

        match first_error {
            None => Ok(()),
            Some(error) => Err(error),
        }
    }

    /// Send a document to a client that doesn't have it yet.
    ///
    /// Finds an existing source for the document, downloads if needed,
    /// then imports to the target client. Automatically invalidates the cache.
    pub async fn send_document_to_client(
        &self,
        document: Document,
        target: ClientSelector,
    ) -> Result<(), FilesClientError> {
        let result = self
            .aggregator
            .read()
            .await
            .send_document_to_client(&document, &target)
            .await;
        self.set_expired().await;
        result.map(|_| ())
    }

    /// Delete a single source of a document.
    ///
    /// Automatically invalidates the cache after the deletion.
    pub async fn delete_document_source(
        &self,
        source: DocumentSource,
        content: DocumentContent,
    ) -> Result<(), FilesClientError> {
        let result = self
            .aggregator
            .read()
            .await
            .delete_document_source(source, content)
            .await;
        self.set_expired().await;
        result
    }

    /// Merge `losers` into `winner`, re-assigning all their file sources.
    ///
    /// Automatically invalidates the cache after the merge.
    pub async fn merge_documents(
        &self,
        winner: &Document,
        losers: &[Document],
    ) -> Result<(), FilesClientError> {
        let result = self
            .aggregator
            .read()
            .await
            .merge_documents(winner, losers)
            .await;
        self.set_expired().await;
        result
    }

    /// Open a document using the system's default application.
    ///
    /// Prefers local sources over remote sources.
    pub async fn open_document(&self, document: Document) -> Result<ExitStatus, FilesClientError> {
        self.aggregator.read().await.xdg_open_file(document).await
    }

    /// Get the list of client selectors.
    ///
    /// Returns the selectors for all registered clients.
    pub async fn get_client_selectors(&self) -> Vec<ClientSelector> {
        self.aggregator.read().await.client_selectors()
    }

    /// Add a client to the aggregator.
    ///
    /// Automatically invalidates the cache after adding.
    pub async fn add_client(&self, client: Client) {
        self.aggregator.write().await.add(client);
        self.set_expired().await;
    }

    /// Remove a client from the aggregator.
    ///
    /// Automatically invalidates the cache after removing.
    pub async fn remove_client(&self, selector: &ClientSelector) {
        self.aggregator.write().await.remove(selector);
        self.set_expired().await;
    }
}

impl HasSetExpired for DocumentProvider {
    async fn set_expired(&self) {
        // Invalidate both caches - observable cache first (notifies subscribers), then tags cache
        self.documents_cache.set_expired().await;
        self.tags_cache.set_expired().await;
    }
}

impl Expiring for DocumentProvider {
    async fn is_expired(&self) -> bool {
        self.documents_cache.is_expired().await || self.tags_cache.is_expired().await
    }
}

impl Provider<Documents> for DocumentProvider {
    type Error = FilesClientError;

    async fn provide(&self) -> Result<Documents, Self::Error> {
        self.get_documents().await
    }
}

impl Provider<Vec<String>> for DocumentProvider {
    type Error = FilesClientError;

    async fn provide(&self) -> Result<Vec<String>, Self::Error> {
        self.get_all_tags().await
    }
}

impl Provider<Vec<ClientSelector>> for DocumentProvider {
    type Error = Infallible;

    async fn provide(&self) -> Result<Vec<ClientSelector>, Self::Error> {
        Ok(self.get_client_selectors().await)
    }
}
