mod cache;
mod expiring_item_cache;
mod expiring_value;
mod fallback_provider;
mod mapping_provider;
mod observable_provider;
mod value;

use std::sync::Arc;

pub use cache::Cache;
pub use expiring_item_cache::ExpiringItemCache;
pub use expiring_value::Expired;
pub use expiring_value::ExpiringValue;
pub use fallback_provider::FallbackProvider;
pub use mapping_provider::MappingProvider;
pub use observable_provider::HasSetExpired;
pub use observable_provider::Invalidated;
pub use observable_provider::ObservableProvider;
use tokio::sync::RwLock;
pub use value::Value;

#[trait_variant::make(Send)]
pub trait Provider<T> {
    type Error;
    async fn provide(&self) -> Result<T, Self::Error>;

    fn map<F>(self, transformation: F) -> MappingProvider<Self, F, T>
    where
        Self: Sized,
    {
        MappingProvider::new(self, transformation)
    }

    fn cache(self) -> Cache<T, Self>
    where
        Self: Sized,
    {
        Cache::new(self)
    }

    fn expiring_item_cache(self) -> ExpiringItemCache<T, Self>
    where
        Self: Sized,
        T: Expiring,
    {
        ExpiringItemCache::new(self)
    }

    /// Wrap this provider in an observable provider that notifies subscribers on invalidation.
    ///
    /// This is useful for reactive UI patterns where the UI needs to refresh when
    /// underlying data changes.
    fn observable(self) -> ObservableProvider<Self>
    where
        Self: Sized,
    {
        ObservableProvider::new(self)
    }

    /// Create a fallback chain with another provider.
    ///
    /// If this provider fails, the fallback provider will be tried.
    /// This is useful for operations that might fail on one source but succeed on another.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let provider = local_provider.or_fallback(remote_provider);
    /// let result = provider.provide().await?;
    /// ```
    fn or_fallback<P>(self, fallback: P) -> FallbackProvider<Self, P>
    where
        Self: Sized,
        P: Provider<T, Error = Self::Error>,
    {
        FallbackProvider::with_fallback(self, fallback)
    }
}

impl<T, P, E> Provider<T> for Arc<P>
where
    P: Provider<T, Error = E> + Sync,
{
    type Error = E;
    async fn provide(&self) -> Result<T, Self::Error> {
        self.as_ref().provide().await
    }
}

impl<T, P, E> Provider<T> for RwLock<P>
where
    P: Provider<T, Error = E> + Sync,
{
    type Error = E;
    async fn provide(&self) -> Result<T, Self::Error> {
        self.read().await.provide().await
    }
}

#[trait_variant::make(Send)]
pub trait Expiring {
    async fn is_expired(&self) -> bool;
}

impl<T> Expiring for Arc<T>
where
    T: Expiring + Sync,
{
    async fn is_expired(&self) -> bool {
        self.as_ref().is_expired().await
    }
}

impl<P> Expiring for RwLock<P>
where
    P: Expiring + Sync,
{
    async fn is_expired(&self) -> bool {
        self.read().await.is_expired().await
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;
    use std::sync::atomic::AtomicU8;
    use std::sync::atomic::Ordering;
    use std::sync::RwLock;

    use super::*;

    static EXPIRED_COUNTER: RwLock<bool> = RwLock::new(true);

    fn set_expired(value: bool) {
        let mut expired = EXPIRED_COUNTER.write().unwrap();
        *expired = value;
    }

    fn get_expired() -> bool {
        let expired = EXPIRED_COUNTER.read().unwrap();
        *expired
    }

    #[derive(Default)]
    struct Counter {
        value: AtomicU8,
    }

    impl Provider<u8> for Counter {
        type Error = Infallible;

        async fn provide(&self) -> Result<u8, Self::Error> {
            let result = self.value.load(Ordering::Relaxed) + 1;
            set_expired(false);
            self.value.store(result, Ordering::Release);
            Ok(result)
        }
    }

    impl Expiring for u8 {
        async fn is_expired(&self) -> bool {
            get_expired()
        }
    }

    #[tokio::test]
    async fn test_value_provider() {
        let actual = Value::from("Hello World!");
        assert_eq!(actual.provide().await.unwrap(), "Hello World!");
    }

    #[tokio::test]
    async fn test_counter() {
        let counter = Counter::default();
        assert_eq!(counter.provide().await.unwrap(), 1);
        assert_eq!(counter.provide().await.unwrap(), 2);
        assert_eq!(counter.provide().await.unwrap(), 3);
        assert_eq!(counter.provide().await.unwrap(), 4);
    }

    #[tokio::test]
    async fn test_counter_double() {
        let counter = Arc::new(Counter::default()).map(|x| x * 2);
        assert_eq!(counter.provide().await.unwrap(), 2);
        assert_eq!(counter.provide().await.unwrap(), 4);
        assert_eq!(counter.provide().await.unwrap(), 6);
        assert_eq!(counter.provide().await.unwrap(), 8);
    }

    #[tokio::test]
    async fn test_cached_provider() {
        let provider = Counter::default().cache();
        assert_eq!(provider.provide().await.unwrap(), 1);
        assert_eq!(provider.provide().await.unwrap(), 1);
        provider.set_expired().await;
        assert_eq!(provider.provide().await.unwrap(), 2);
        assert_eq!(provider.provide().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn test_expiring_cache_provider() {
        let provider = Counter::default().expiring_item_cache();
        assert_eq!(provider.provide().await.unwrap(), 1);
        assert_eq!(provider.provide().await.unwrap(), 1);

        set_expired(true);

        assert_eq!(provider.provide().await.unwrap(), 2);
        assert_eq!(provider.provide().await.unwrap(), 2);
    }
}
