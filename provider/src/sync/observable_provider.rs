//! Observable provider that notifies subscribers when the cache is invalidated.
//!
//! This module provides an `ObservableProvider` that wraps another provider and
//! emits notifications when `set_expired()` is called. This is useful for reactive
//! UI patterns where the UI needs to refresh when underlying data changes.

use std::sync::mpsc;

use crate::sync::Expiring;
use crate::sync::HasSetExpired;
use crate::sync::Invalidated;
use crate::sync::Observable;
use crate::sync::Provider;
use crate::sync::broadcaster::Broadcaster;

/// An observable provider that notifies subscribers when invalidated.
///
/// This wraps another provider and adds the ability to subscribe to invalidation
/// notifications. When `set_expired()` is called, all subscribers receive a
/// notification.
///
/// # Example
///
/// ```ignore
/// use provider::sync::{Provider, Cache, ObservableProvider};
///
/// let cache = some_provider.cache();
/// let observable = ObservableProvider::new(cache);
///
/// // Subscribe to invalidation notifications
/// let rx = observable.subscribe();
///
/// // In another thread, listen for notifications
/// std::thread::spawn(move || {
///     while rx.recv().is_ok() {
///         println!("Cache was invalidated!");
///     }
/// });
///
/// // When set_expired is called, subscribers are notified
/// observable.set_expired();
/// ```
pub struct ObservableProvider<P> {
    provider: P,
    broadcaster: Broadcaster<Invalidated>,
}

impl<P> ObservableProvider<P> {
    /// Create a new observable provider wrapping the given provider.
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            broadcaster: Broadcaster::new(),
        }
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
    fn subscribe(&self) -> mpsc::Receiver<Invalidated> {
        self.broadcaster.subscribe()
    }
}

impl<P> ObservableProvider<P>
where
    P: HasSetExpired,
{
    /// Invalidate the cache and notify all subscribers.
    pub fn set_expired(&self) {
        self.provider.set_expired();
        self.broadcaster.send(Invalidated);
    }
}

impl<T, P, E> Provider<T> for ObservableProvider<P>
where
    P: Provider<T, Error = E>,
{
    type Error = E;

    fn provide(&self) -> Result<T, Self::Error> {
        self.provider.provide()
    }
}

impl<P> Expiring for ObservableProvider<P>
where
    P: Expiring,
{
    fn is_expired(&self) -> bool {
        self.provider.is_expired()
    }
}
