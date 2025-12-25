use std::convert::identity;
use std::marker::PhantomData;

use tokio::sync::broadcast;
use tokio::sync::RwLock;

use crate::r#async::Expiring;
use crate::r#async::HasSetExpired;
use crate::r#async::Invalidated;
use crate::r#async::Observable;
use crate::r#async::Provider;

/// An observable cache that notifies subscribers when invalidated.
pub struct ObservableCache<P, F, T, R> {
    provider: P,
    transformation: F,
    value: RwLock<Option<R>>,
    sender: broadcast::Sender<Invalidated>,
    _marker: PhantomData<T>,
}

impl<P, T> ObservableCache<P, fn(T) -> T, T, T> {
    pub fn new(provider: P) -> Self {
        Self::new_transform(provider, identity)
    }
}

impl<P, F, T, R> ObservableCache<P, F, T, R> {
    pub fn new_transform(provider: P, transformation: F) -> Self {
        Self::with_capacity(provider, transformation, 16)
    }

    pub fn with_capacity(provider: P, transformation: F, capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            provider,
            transformation,
            value: Default::default(),
            sender,
            _marker: PhantomData,
        }
    }
}

impl<P, F, T, R> Observable<Invalidated> for ObservableCache<P, F, T, R> {
    fn subscribe(&self) -> broadcast::Receiver<Invalidated> {
        self.sender.subscribe()
    }
}

impl<P, F, T, R> HasSetExpired for ObservableCache<P, F, T, R>
where
    P: Provider<T> + Sync,
    F: Send + Sync,
    T: Send + Sync,
    R: Send + Sync,
{
    /// Invalidate the cache and notify all subscribers.
    async fn set_expired(&self) {
        let mut value = self.value.write().await;
        *value = None;
        // Ignore send errors - they just mean no receivers are listening
        let _ = self.sender.send(Invalidated);
    }
}

impl<P, F, T, R> Provider<R> for ObservableCache<P, F, T, R>
where
    P: Provider<T> + Sync,
    F: Fn(T) -> R + Send + Sync,
    T: Send + Sync,
    R: Clone + Send + Sync,
{
    type Error = P::Error;

    async fn provide(&self) -> Result<R, Self::Error> {
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
        tracing::debug!("apply transformation");
        let new_value = (self.transformation)(new_value);
        tracing::debug!("store retrieved value in cache");
        *value = Some(new_value.clone());
        tracing::debug!("return retrieved value");
        Ok(new_value)
    }
}

impl<P, F, T, R> Expiring for ObservableCache<P, F, T, R>
where
    P: Provider<T> + Sync,
    F: Send + Sync,
    T: Send + Sync,
    R: Send + Sync,
{
    async fn is_expired(&self) -> bool {
        self.value.read().await.is_none()
    }
}
