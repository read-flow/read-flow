mod and_then;
mod broadcaster;
mod cache;
mod expiring_item_cache;
mod expiring_value;
mod fallback_provider;
mod map;
mod observable_cache;
mod observable_provider;
mod value;

use std::sync::Arc;
use std::sync::RwLock;
use std::sync::mpsc;

pub use and_then::AndThen;
pub use cache::Cache;
pub use expiring_item_cache::ExpiringItemCache;
pub use expiring_value::Expired;
pub use expiring_value::ExpiringValue;
pub use fallback_provider::FallbackProvider;
pub use map::Map;
pub use observable_cache::ObservableCache;
pub use observable_provider::ObservableProvider;
pub use value::Value;

pub trait Provider<T> {
    type Error;
    fn provide(&self) -> Result<T, Self::Error>;

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
    /// let result = provider.provide()?;
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
    P: Provider<T, Error = E>,
{
    type Error = E;
    fn provide(&self) -> Result<T, Self::Error> {
        self.as_ref().provide()
    }
}

impl<T, P, E> Provider<T> for RwLock<P>
where
    P: Provider<T, Error = E>,
{
    type Error = E;
    fn provide(&self) -> Result<T, Self::Error> {
        self.read().unwrap().provide()
    }
}

impl<T, E, F> Provider<T> for F
where
    F: Fn() -> Result<T, E>,
{
    type Error = E;
    fn provide(&self) -> Result<T, Self::Error> {
        self()
    }
}

pub trait Expiring {
    fn is_expired(&self) -> bool;
}

impl<T> Expiring for Arc<T>
where
    T: Expiring,
{
    fn is_expired(&self) -> bool {
        self.as_ref().is_expired()
    }
}

impl<P> Expiring for RwLock<P>
where
    P: Expiring,
{
    fn is_expired(&self) -> bool {
        self.read().unwrap().is_expired()
    }
}

/// Trait for providers that have a `set_expired` method.
///
/// This is implemented by `Cache` and `ExpiringItemCache`.
pub trait HasSetExpired {
    /// Invalidate the cached value.
    fn set_expired(&self);
}

// Implement HasSetExpired for Arc<T> where T: HasSetExpired
impl<T> HasSetExpired for std::sync::Arc<T>
where
    T: HasSetExpired,
{
    fn set_expired(&self) {
        self.as_ref().set_expired()
    }
}

/// A notification that the observable provider's cache has been invalidated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Invalidated;

pub trait Observable<T> {
    fn subscribe(&self) -> mpsc::Receiver<T>;
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::AtomicU8;
    use std::sync::atomic::Ordering;

    use super::*;

    #[derive(Default)]
    struct Counter {
        value: AtomicU8,
    }

    impl Provider<u8> for Counter {
        type Error = Infallible;

        fn provide(&self) -> Result<u8, Self::Error> {
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
        fn is_expired(&self) -> bool {
            self.expired.load(Ordering::Acquire)
        }
    }

    impl Provider<CounterValue> for ExpiringCounter {
        type Error = Infallible;

        fn provide(&self) -> Result<CounterValue, Self::Error> {
            let result = self.value.load(Ordering::Relaxed) + 1;
            self.expired.store(false, Ordering::Release);
            self.value.store(result, Ordering::Release);
            Ok(CounterValue {
                value: result,
                expired: self.expired.clone(),
            })
        }
    }

    #[test]
    fn test_value_provider() {
        let actual = Value::from("Hello World!");
        assert_eq!(actual.provide().unwrap(), "Hello World!");
    }

    #[test]
    fn test_counter() {
        let counter = Counter::default();
        assert_eq!(counter.provide().unwrap(), 1);
        assert_eq!(counter.provide().unwrap(), 2);
        assert_eq!(counter.provide().unwrap(), 3);
        assert_eq!(counter.provide().unwrap(), 4);
    }

    #[test]
    fn test_counter_double() {
        let counter = Arc::new(Counter::default()).map(|x| x * 2);
        assert_eq!(counter.provide().unwrap(), 2);
        assert_eq!(counter.provide().unwrap(), 4);
        assert_eq!(counter.provide().unwrap(), 6);
        assert_eq!(counter.provide().unwrap(), 8);
    }

    #[test]
    fn test_cached_provider() {
        let provider = Counter::default().cache();
        assert_eq!(provider.provide().unwrap(), 1);
        assert_eq!(provider.provide().unwrap(), 1);
        provider.set_expired();
        assert_eq!(provider.provide().unwrap(), 2);
        assert_eq!(provider.provide().unwrap(), 2);
    }

    #[test]
    fn test_expiring_cache_provider() {
        let (counter, expired_flag) = ExpiringCounter::new();
        let provider = counter.expiring_item_cache();
        assert_eq!(provider.provide().unwrap().value, 1);
        assert_eq!(provider.provide().unwrap().value, 1);

        expired_flag.store(true, Ordering::Release);

        assert_eq!(provider.provide().unwrap().value, 2);
        assert_eq!(provider.provide().unwrap().value, 2);
    }
}
