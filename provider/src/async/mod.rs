mod and_then;
mod cache;
mod expiring_item_cache;
mod expiring_value;
mod fallback_provider;
mod map;
mod observable_cache;
mod observable_provider;
mod value;

use std::sync::Arc;

pub use and_then::AndThen;
pub use cache::Cache;
pub use expiring_item_cache::ExpiringItemCache;
pub use expiring_value::Expired;
pub use expiring_value::ExpiringValue;
pub use fallback_provider::FallbackProvider;
pub use map::Map;
pub use observable_cache::ObservableCache;
pub use observable_provider::ObservableProvider;
use tokio::sync::RwLock;
use tokio::sync::broadcast;
pub use value::Value;

#[trait_variant::make(Send)]
pub trait Provider<T> {
    type Error;
    async fn provide(&self) -> Result<T, Self::Error>;

    fn map<F>(self, transformation: F) -> Map<Self, F, T>
    where
        Self: Sized,
    {
        Map::new(self, transformation)
    }

    fn and_then<F>(self, transformation: F) -> AndThen<Self, F, T>
    where
        Self: Sized,
    {
        AndThen::new(self, transformation)
    }

    fn observable_cache(self) -> ObservableCache<Self, fn(T) -> T, T, T>
    where
        Self: Sized,
    {
        ObservableCache::new(self)
    }

    fn arc(self) -> Arc<Self>
    where
        Self: Sized,
    {
        Arc::new(self)
    }

    fn observable_cache_with_transform<F, R>(
        self,
        transformation: F,
    ) -> ObservableCache<Self, F, T, R>
    where
        Self: Sized,
    {
        ObservableCache::with_transform(self, transformation)
    }

    fn observable_cache_with_fn<R>(
        self,
        transformation: fn(T) -> R,
    ) -> ObservableCache<Self, fn(T) -> R, T, R>
    where
        Self: Sized,
    {
        ObservableCache::with_transform(self, transformation)
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

/// Trait for providers that have a `set_expired` method.
///
/// This is implemented by `Cache` and `ExpiringItemCache`.
#[trait_variant::make(Send)]
pub trait HasSetExpired {
    /// Invalidate the cached value.
    async fn set_expired(&self);
}

// Implement HasSetExpired for Arc<T> where T: HasSetExpired
impl<T> HasSetExpired for std::sync::Arc<T>
where
    T: HasSetExpired + Sync,
{
    async fn set_expired(&self) {
        self.as_ref().set_expired().await
    }
}

/// A notification that the observable provider's cache has been invalidated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Invalidated;

pub trait Observable<T> {
    fn subscribe(&self) -> broadcast::Receiver<T>;
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::AtomicU8;
    use std::sync::atomic::Ordering;

    use assert4rs::Assert;

    use super::*;

    #[derive(Default)]
    struct Counter {
        value: AtomicU8,
    }

    impl Provider<u8> for Counter {
        type Error = Infallible;

        async fn provide(&self) -> Result<u8, Self::Error> {
            let result = self.value.load(Ordering::Relaxed) + 1;
            self.value.store(result, Ordering::Release);
            Ok(result)
        }
    }

    /// A counter whose cached values carry their own expiry flag, avoiding
    /// any shared global state between parallel tests.
    struct ExpiringCounter {
        value: AtomicU8,
        expired: Arc<AtomicBool>,
    }

    impl ExpiringCounter {
        fn new() -> (Self, Arc<AtomicBool>) {
            let flag = Arc::new(AtomicBool::new(false));
            (
                Self {
                    value: AtomicU8::new(0),
                    expired: flag.clone(),
                },
                flag,
            )
        }
    }

    #[derive(Clone)]
    struct CounterValue {
        value: u8,
        expired: Arc<AtomicBool>,
    }

    impl Expiring for CounterValue {
        async fn is_expired(&self) -> bool {
            self.expired.load(Ordering::Acquire)
        }
    }

    impl Provider<CounterValue> for ExpiringCounter {
        type Error = Infallible;

        async fn provide(&self) -> Result<CounterValue, Self::Error> {
            let result = self.value.load(Ordering::Relaxed) + 1;
            self.expired.store(false, Ordering::Release);
            self.value.store(result, Ordering::Release);
            Ok(CounterValue {
                value: result,
                expired: self.expired.clone(),
            })
        }
    }

    #[tokio::test]
    async fn test_value_provider() {
        let actual = Value::from("Hello World!");
        Assert::that(actual.provide().await.unwrap()).is("Hello World!");
    }

    #[tokio::test]
    async fn test_counter() {
        let counter = Counter::default();
        Assert::that(counter.provide().await.unwrap()).is(1);
        Assert::that(counter.provide().await.unwrap()).is(2);
        Assert::that(counter.provide().await.unwrap()).is(3);
        Assert::that(counter.provide().await.unwrap()).is(4);
    }

    #[tokio::test]
    async fn test_counter_double() {
        let counter = Arc::new(Counter::default()).map(|x| x * 2);
        Assert::that(counter.provide().await.unwrap()).is(2);
        Assert::that(counter.provide().await.unwrap()).is(4);
        Assert::that(counter.provide().await.unwrap()).is(6);
        Assert::that(counter.provide().await.unwrap()).is(8);
    }

    #[tokio::test]
    async fn test_cached_provider() {
        let provider = Counter::default().cache();
        Assert::that(provider.provide().await.unwrap()).is(1);
        Assert::that(provider.provide().await.unwrap()).is(1);
        provider.set_expired().await;
        Assert::that(provider.provide().await.unwrap()).is(2);
        Assert::that(provider.provide().await.unwrap()).is(2);
    }

    #[tokio::test]
    async fn test_expiring_cache_provider() {
        let (counter, expired_flag) = ExpiringCounter::new();
        let provider = counter.expiring_item_cache();
        Assert::that(provider.provide().await.unwrap().value).is(1);
        Assert::that(provider.provide().await.unwrap().value).is(1);

        expired_flag.store(true, Ordering::Release);

        Assert::that(provider.provide().await.unwrap().value).is(2);
        Assert::that(provider.provide().await.unwrap().value).is(2);
    }

    #[tokio::test]
    async fn test_and_then_transforms_value() {
        let provider = Value::from(6u8).and_then(|x: u8| -> Result<u8, Infallible> { Ok(x * 7) });
        Assert::that(provider.provide().await.unwrap()).is(42u8);
    }

    #[tokio::test]
    async fn test_and_then_doubles_via_map_then_and_then() {
        // Chain map (infallible) then and_then (fallible) to verify composition
        let provider = Arc::new(Counter::default())
            .map(|x: u8| x * 2)
            .and_then(|x: u8| -> Result<u8, Infallible> { Ok(x + 1) });
        Assert::that(provider.provide().await.unwrap()).is(3); // (1*2)+1
        Assert::that(provider.provide().await.unwrap()).is(5); // (2*2)+1
    }

    #[tokio::test]
    async fn test_observable_cache_caches_value() {
        let provider = Counter::default().observable_cache();
        Assert::that(provider.provide().await.unwrap()).is(1);
        Assert::that(provider.provide().await.unwrap()).is(1); // cached, no second call
    }

    #[tokio::test]
    async fn test_observable_cache_notifies_on_invalidation() {
        let provider = Arc::new(Counter::default().observable_cache());
        let mut rx = provider.subscribe();
        provider.set_expired().await;
        let msg = rx.try_recv().expect("should receive Invalidated");
        Assert::that(msg).is(Invalidated);
    }

    #[tokio::test]
    async fn test_observable_cache_refetches_after_invalidation() {
        let provider = Counter::default().observable_cache();
        Assert::that(provider.provide().await.unwrap()).is(1);
        provider.set_expired().await;
        Assert::that(provider.provide().await.unwrap()).is(2); // new value after invalidation
    }

    #[tokio::test]
    async fn test_observable_cache_is_expired_after_invalidation() {
        let provider = Counter::default().observable_cache();
        provider.provide().await.unwrap(); // populate cache
        Assert::that(provider.is_expired().await).is(false);
        provider.set_expired().await;
        Assert::that(provider.is_expired().await).is(true);
    }

    #[tokio::test]
    async fn test_observable_cache_with_transform() {
        let provider = Counter::default().observable_cache_with_transform(|x: u8| x * 10);
        Assert::that(provider.provide().await.unwrap()).is(10);
        Assert::that(provider.provide().await.unwrap()).is(10); // cached
        provider.set_expired().await;
        Assert::that(provider.provide().await.unwrap()).is(20); // re-fetched and transformed
    }

    #[tokio::test]
    async fn test_arc_provider_delegates() {
        let provider = Arc::new(Counter::default());
        Assert::that(provider.provide().await.unwrap()).is(1);
        Assert::that(provider.provide().await.unwrap()).is(2);
    }
}
