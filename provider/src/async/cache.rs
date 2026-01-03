use std::any::type_name;
use std::fmt;

use tokio::sync::RwLock;

use crate::r#async::Expiring;
use crate::r#async::HasSetExpired;
use crate::r#async::Provider;

/// Cache
pub struct Cache<T, P> {
    provider: P,
    value: RwLock<Option<T>>,
}

impl<T, P> fmt::Debug for Cache<T, P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Cache Provider of `{}`", type_name::<T>())
    }
}

impl<T, P> Cache<T, P> {
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            value: RwLock::new(None),
        }
    }

    pub fn provider(&self) -> &P {
        &self.provider
    }

    pub fn provider_mut(&mut self) -> &mut P {
        &mut self.provider
    }
}

impl<T, P, E> Provider<T> for Cache<T, P>
where
    P: Provider<T, Error = E> + Sync,
    T: Clone + Send + Sync,
{
    type Error = E;

    async fn provide(&self) -> Result<T, Self::Error> {
        // Try to read the cached value first
        {
            let value = self.value.read().await;
            if let Some(ref cached) = *value {
                tracing::debug!("return value from cache, after read lock");
                return Ok(cached.clone());
            }
        }

        // Value not cached, acquire write lock and populate
        let mut value = self.value.write().await;
        // Double-check after acquiring write lock
        if let Some(ref cached) = *value {
            tracing::debug!("return value from cache, after write lock");
            return Ok(cached.clone());
        }

        tracing::debug!("retrieve value from provider");
        let new_value = self.provider.provide().await?;
        tracing::debug!("store retrieved value in cache");
        *value = Some(new_value.clone());
        tracing::debug!("return retrieved value");
        Ok(new_value)
    }
}

impl<T, P> Expiring for Cache<T, P>
where
    P: Provider<T> + Sync,
    T: Send + Sync,
{
    async fn is_expired(&self) -> bool {
        self.value.read().await.is_none()
    }
}

// Implement HasSetExpired for Cache
impl<T, P> HasSetExpired for Cache<T, P>
where
    P: Send + Sync,
    T: Send + Sync,
{
    async fn set_expired(&self) {
        let mut value = self.value.write().await;
        *value = None;
    }
}
