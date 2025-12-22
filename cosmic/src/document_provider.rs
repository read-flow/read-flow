use std::any::Any;
use std::collections::HashSet;
use std::process::ExitStatus;
use std::sync::Arc;

use cosmic::iced::Subscription;
use provider::r#async::Cache;
use provider::r#async::Invalidated;
use provider::r#async::MappingProvider;
use provider::r#async::ObservableProvider;
use provider::r#async::Provider;
use tokio::sync::RwLock;
use tokio::sync::broadcast;

use crate::aggregator::Aggregator;
use crate::aggregator::Document;
use crate::aggregator::Documents;
use crate::client::Client;
use crate::client::ClientSelector;
use crate::client::FilesClientError;

type DocumentCache = Arc<Cache<Documents, Arc<RwLock<Aggregator>>>>;
type ObservableDocumentCache = Arc<ObservableProvider<DocumentCache>>;
type TagsCache = Cache<
    Vec<String>,
    MappingProvider<ObservableDocumentCache, fn(Documents) -> Vec<String>, Documents>,
>;

/// Extract unique sorted tags from documents.
fn extract_tags(documents: Documents) -> Vec<String> {
    let mut tags = documents
        .into_iter()
        .flat_map(|document| document.metadata.tags)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    tags.sort();
    tags
}

pub struct DocumentProvider {
    pub(crate) aggregator: Arc<RwLock<Aggregator>>,
    observable_cache: ObservableDocumentCache,
    tags_cache: TagsCache,
}

impl DocumentProvider {
    pub fn new(aggregator: Aggregator) -> Self {
        let aggregator = Arc::new(RwLock::new(aggregator));
        let document_cache = Arc::new(aggregator.clone().cache());
        let observable_cache = Arc::new(ObservableProvider::new(document_cache));
        let tags_cache = observable_cache
            .clone()
            .map(extract_tags as fn(Documents) -> Vec<String>)
            .cache();
        Self {
            aggregator,
            observable_cache,
            tags_cache,
        }
    }

    pub async fn set_expired(&self) {
        // Invalidate both caches - observable cache first (notifies subscribers), then tags cache
        self.observable_cache.set_expired().await;
        self.tags_cache.set_expired().await;
    }

    pub async fn get_documents(&self) -> Result<Documents, FilesClientError> {
        self.observable_cache.provide().await
    }

    /// Subscribe to cache invalidation notifications.
    ///
    /// Returns a receiver that will receive notifications whenever the cache is invalidated.
    /// This can be used to trigger UI refreshes when data changes.
    pub fn subscribe(&self) -> broadcast::Receiver<Invalidated> {
        self.observable_cache.subscribe()
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
        F: Fn() -> M + Send + 'static,
    {
        use cosmic::iced_futures::futures::SinkExt;
        use cosmic::iced_futures::futures::channel::mpsc;

        let mut receiver = self.subscribe();
        Subscription::run_with_id(
            Invalidated.type_id(),
            cosmic::iced::stream::channel(4, move |mut sender: mpsc::Sender<M>| async move {
                loop {
                    match receiver.recv().await {
                        Ok(_) => {
                            if sender.send(f()).await.is_err() {
                                // Channel closed, stop the subscription
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            // Sender dropped, stop the subscription
                            break;
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            // Missed some messages, but continue listening
                            // Still send a notification since data has changed
                            if sender.send(f()).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            }),
        )
    }

    /// Get all unique tags from all documents.
    ///
    /// Uses a cached mapping provider that derives tags from the document cache.
    /// The cache is automatically invalidated when the document cache expires.
    pub async fn get_all_tags(&self) -> Result<Vec<String>, FilesClientError> {
        self.tags_cache.provide().await
    }

    /// Get a single document by fingerprint.
    ///
    /// Uses the cached documents to efficiently look up a single document.
    pub async fn get_document(
        &self,
        fingerprint: &str,
    ) -> Result<Option<Document>, FilesClientError> {
        self.get_documents()
            .await
            .map(|docs| docs.get(fingerprint).cloned())
    }

    /// Update a document across all sources.
    ///
    /// Automatically invalidates the cache after the update.
    pub async fn update_document(&self, document: Document) -> Result<(), FilesClientError> {
        let result = self.aggregator.read().await.update_document(document).await;
        self.set_expired().await;
        result
    }

    /// Add tags to a document across all sources.
    ///
    /// Automatically invalidates the cache after the update.
    pub async fn add_document_tags(
        &self,
        document: Document,
        tags: Vec<String>,
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

    /// Delete tags from a document across all sources.
    ///
    /// Automatically invalidates the cache after the update.
    pub async fn delete_document_tags(
        &self,
        document: Document,
        tags: Vec<String>,
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

// impl Provider<Documents> for DocumentProvider {
//     type Error = FilesClientError;

//     async fn provide(&self) -> Result<Documents, Self::Error> {
//         self.get_documents().await
//     }
// }

// impl Expiring for DocumentProvider {
//     async fn is_expired(&self) -> bool {
//         self.document_cache.is_expired().await
//     }
// }

// impl Provider<Vec<String>> for DocumentProvider {
//     type Error = FilesClientError;

//     async fn provide(&self) -> Result<Vec<String>, Self::Error> {
//         self.get_all_tags().await
//     }
// }
