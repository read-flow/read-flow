use std::any::type_name;
use std::convert::identity;
use std::fmt;
use std::marker::PhantomData;
use std::sync::RwLock;
use std::sync::mpsc;

use crate::sync::Expiring;
use crate::sync::HasSetExpired;
use crate::sync::Invalidated;
use crate::sync::Observable;
use crate::sync::Provider;
use crate::sync::broadcaster::Broadcaster;

/// An observable cache that notifies subscribers when invalidated.
pub struct ObservableCache<P, F, T, R> {
    provider: P,
    transformation: F,
    value: RwLock<Option<R>>,
    broadcaster: Broadcaster<Invalidated>,
    _marker: PhantomData<T>,
}

impl<P, F, T, R> fmt::Debug for ObservableCache<P, F, T, R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ObservableCache of `{}`", type_name::<R>())
    }
}

impl<P, T> ObservableCache<P, fn(T) -> T, T, T> {
    pub fn new(provider: P) -> Self {
        Self::with_transform(provider, identity)
    }
}

impl<P, F, T, R> ObservableCache<P, F, T, R> {
    pub fn with_transform(provider: P, transformation: F) -> Self {
        Self {
            provider,
            transformation,
            value: Default::default(),
            broadcaster: Broadcaster::new(),
            _marker: PhantomData,
        }
    }
}

impl<P, F, T, R> Observable<Invalidated> for ObservableCache<P, F, T, R> {
    fn subscribe(&self) -> mpsc::Receiver<Invalidated> {
        self.broadcaster.subscribe()
    }
}

impl<P, F, T, R> HasSetExpired for ObservableCache<P, F, T, R> {
    /// Invalidate the cache and notify all subscribers.
    fn set_expired(&self) {
        let mut value = self.value.write().unwrap();
        *value = None;
        self.broadcaster.send(Invalidated);
    }
}

impl<P, F, T, R> Provider<R> for ObservableCache<P, F, T, R>
where
    P: Provider<T>,
    F: Fn(T) -> R,
    R: Clone,
{
    type Error = P::Error;

    fn provide(&self) -> Result<R, Self::Error> {
        // Try to read the cached value first
        {
            let value = self.value.read().unwrap();
            if let Some(ref cached) = *value {
                tracing::debug!("return value from cache, after read lock");
                return Ok(cached.clone());
            }
        }

        // Value not cached, acquire write lock and populate
        let mut value = self.value.write().unwrap();
        // Double-check after acquiring write lock
        if let Some(ref cached) = *value {
            tracing::debug!("return value from cache, after write lock");
            return Ok(cached.clone());
        }

        tracing::debug!("retrieve value from provider");
        let new_value = self.provider.provide()?;
        tracing::debug!("apply transformation");
        let new_value = (self.transformation)(new_value);
        tracing::debug!("store retrieved value in cache");
        *value = Some(new_value.clone());
        tracing::debug!("return retrieved value");
        Ok(new_value)
    }
}

impl<P, F, T, R> Expiring for ObservableCache<P, F, T, R> {
    fn is_expired(&self) -> bool {
        self.value.read().unwrap().is_none()
    }
}
