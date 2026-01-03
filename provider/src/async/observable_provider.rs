//! Observable provider that notifies subscribers when the cache is invalidated.
//!
//! This module provides an `ObservableProvider` that wraps another provider and
//! emits notifications when `set_expired()` is called. This is useful for reactive
//! UI patterns where the UI needs to refresh when underlying data changes.

use tokio::sync::broadcast;

use crate::r#async::Expiring;
use crate::r#async::HasSetExpired;
use crate::r#async::Invalidated;
use crate::r#async::Observable;
use crate::r#async::Provider;

/// An observable provider that notifies subscribers when invalidated.
///
/// This wraps another provider and adds the ability to subscribe to invalidation
/// notifications. When `set_expired()` is called, all subscribers receive a
/// notification.
///
/// # Example
///
/// ```ignore
/// use provider::r#async::{Provider, Cache, ObservableProvider};
///
/// let cache = some_provider.cache();
/// let observable = ObservableProvider::new(cache);
///
/// // Subscribe to invalidation notifications
/// let mut rx = observable.subscribe();
///
/// // In another task, listen for notifications
/// tokio::spawn(async move {
///     while rx.recv().await.is_ok() {
///         println!("Cache was invalidated!");
///     }
/// });
///
/// // When set_expired is called, subscribers are notified
/// observable.set_expired().await;
/// ```
pub struct ObservableProvider<P> {
    provider: P,
    sender: broadcast::Sender<Invalidated>,
}

impl<P> ObservableProvider<P> {
    /// Create a new observable provider wrapping the given provider.
    ///
    /// The channel capacity determines how many invalidation notifications
    /// can be buffered before slow receivers start missing messages.
    pub fn new(provider: P) -> Self {
        Self::with_capacity(provider, 16)
    }

    /// Create a new observable provider with a specific channel capacity.
    pub fn with_capacity(provider: P, capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { provider, sender }
    }

    /// Get a reference to the underlying provider.
    pub fn provider(&self) -> &P {
        &self.provider
    }

    /// Get a mutable reference to the underlying provider.
    pub fn provider_mut(&mut self) -> &mut P {
        &mut self.provider
    }
}

impl<P> Observable<Invalidated> for ObservableProvider<P> {
    /// Subscribe to invalidation notifications.
    ///
    /// Returns a receiver that will receive `Invalidated` messages whenever
    /// `set_expired()` is called on this provider.
    fn subscribe(&self) -> broadcast::Receiver<Invalidated> {
        self.sender.subscribe()
    }
}

impl<P> ObservableProvider<P>
where
    P: HasSetExpired,
{
    /// Invalidate the cache and notify all subscribers.
    pub async fn set_expired(&self) {
        self.provider.set_expired().await;
        // Ignore send errors - they just mean no receivers are listening
        let _ = self.sender.send(Invalidated);
    }
}

impl<T, P, E> Provider<T> for ObservableProvider<P>
where
    P: Provider<T, Error = E> + Sync,
    T: Send,
{
    type Error = E;

    async fn provide(&self) -> Result<T, Self::Error> {
        self.provider.provide().await
    }
}

impl<P> Expiring for ObservableProvider<P>
where
    P: Expiring + Sync,
{
    async fn is_expired(&self) -> bool {
        self.provider.is_expired().await
    }
}
