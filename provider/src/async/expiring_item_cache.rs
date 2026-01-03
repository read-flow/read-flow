use tokio::sync::RwLock;

use crate::r#async::Expiring;
use crate::r#async::HasSetExpired;
use crate::r#async::Provider;

pub struct ExpiringItemCache<T, P> {
    provider: P,
    value: RwLock<Option<T>>,
}

impl<T, P> ExpiringItemCache<T, P> {
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            value: RwLock::new(None),
        }
    }
}

impl<T, P> Expiring for ExpiringItemCache<T, P>
where
    P: Send + Sync,
    T: Expiring + Sync,
{
    async fn is_expired(&self) -> bool {
        let value = self.value.read().await;
        match &*value {
            Some(v) => v.is_expired().await,
            None => true,
        }
    }
}

// Implement HasSetExpired for ExpiringItemCache
impl<T, P> HasSetExpired for ExpiringItemCache<T, P>
where
    P: Send + Sync,
    T: Send + Sync,
{
    async fn set_expired(&self) {
        let mut value = self.value.write().await;
        *value = None;
    }
}

impl<T, P, E> Provider<T> for ExpiringItemCache<T, P>
where
    P: Provider<T, Error = E> + Sync,
    T: Expiring + Clone + Sync,
{
    type Error = E;

    async fn provide(&self) -> Result<T, Self::Error> {
        // Try to read the cached value first
        {
            let value = self.value.read().await;
            if let Some(ref cached) = *value {
                if !cached.is_expired().await {
                    return Ok(cached.clone());
                }
            }
        }

        // Value not cached or expired, acquire write lock and populate
        let mut value = self.value.write().await;

        // Double-check after acquiring write lock
        if let Some(ref cached) = *value {
            if !cached.is_expired().await {
                return Ok(cached.clone());
            }
        }

        let new_value = self.provider.provide().await?;
        *value = Some(new_value.clone());
        Ok(new_value)
    }
}
