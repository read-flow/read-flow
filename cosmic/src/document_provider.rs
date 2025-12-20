use std::collections::HashSet;
use std::sync::Arc;

use provider::r#async::Cache;
use provider::r#async::Expiring;
use provider::r#async::Provider;
use tokio::sync::RwLock;

use crate::aggregator::Aggregator;
use crate::aggregator::Documents;
use crate::client::FilesClientError;

type DocumentCache = Arc<Cache<Documents, Arc<RwLock<Aggregator>>>>;

pub struct DocumentProvider {
    pub(crate) aggregator: Arc<RwLock<Aggregator>>,
    document_cache: DocumentCache,
    cached_tags: RwLock<Option<Vec<String>>>,
}

impl DocumentProvider {
    pub fn new(aggregator: Aggregator) -> Self {
        let aggregator = Arc::new(RwLock::new(aggregator));
        let document_cache = Arc::new(aggregator.clone().cache());
        Self {
            aggregator,
            document_cache,
            cached_tags: Default::default(),
        }
    }

    pub async fn set_expired(&self) {
        // First clear aggregator, to ensure cached_tags are not refreshed before.
        self.document_cache.set_expired().await;

        let mut cached_tags = self.cached_tags.write().await;
        *cached_tags = None;
    }

    pub async fn get_documents(&self) -> Result<Documents, FilesClientError> {
        self.document_cache.provide().await
    }

    pub async fn get_all_tags(&self) -> Result<Vec<String>, FilesClientError> {
        {
            let cached_tags = self.cached_tags.read().await;
            if !cached_tags.is_none() && !self.document_cache.is_expired().await {
                // unwrap is safe, because tags are not expired
                return Ok(cached_tags.as_ref().unwrap().clone());
            }
        }

        let mut cached_tags = self.cached_tags.write().await;
        // Double check after aquiring write lock
        if !cached_tags.is_none() && !self.document_cache.is_expired().await {
            // unwrap is safe, because tags are not expired
            return Ok(cached_tags.as_ref().unwrap().clone());
        }

        let documents = self.get_documents().await?;
        let mut tags = documents
            .into_iter()
            .flat_map(|document| document.metadata.tags)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        tags.sort();
        *cached_tags = Some(tags.clone());
        Ok(tags)
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
