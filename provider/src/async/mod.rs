use std::fmt::Debug;
use std::ops::Deref;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use tokio::sync::RwLock;

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
        let provider = self.read().await;
        provider.provide().await
    }
}

pub struct MappingProvider<P, F, T> {
    provider: P,
    transformation: F,
    _marker: std::marker::PhantomData<T>,
}

impl<P, F, T> MappingProvider<P, F, T> {
    pub fn new(provider: P, transformation: F) -> Self {
        Self {
            provider,
            transformation,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T, P, R, F> Provider<R> for MappingProvider<P, F, T>
where
    P: Provider<T> + Sync,
    F: Fn(T) -> R + Send + Sync,
    T: Send + Sync,
{
    type Error = P::Error;
    async fn provide(&self) -> Result<R, Self::Error> {
        self.provider.provide().await.map(&self.transformation)
    }
}

impl<P, F, T> Expiring for MappingProvider<P, F, T>
where
    P: Expiring + Sync,
    F: Send + Sync,
    T: Send + Sync,
{
    async fn is_expired(&self) -> bool {
        self.provider.is_expired().await
    }
}

/// Value
// TODO: nutype?
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Value<T>(T);

impl<T> Value<T> {
    pub const fn new(value: T) -> Self {
        Value(value)
    }
}

impl<T> Default for Value<T>
where
    T: Default,
{
    fn default() -> Self {
        Value(Default::default())
    }
}

impl<T> AsRef<T> for Value<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T> AsMut<T> for Value<T> {
    fn as_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T> From<T> for Value<T> {
    fn from(value: T) -> Self {
        Value(value)
    }
}

impl<T> Provider<T> for Value<T>
where
    T: Clone + Send + Sync,
{
    type Error = ();
    async fn provide(&self) -> Result<T, Self::Error> {
        Ok(self.0.clone())
    }
}

/// Expiring Value
pub struct ExpiringValue<T> {
    value: T,
    expired: AtomicBool,
}

impl<T> ExpiringValue<T> {
    pub fn new(value: T) -> Self {
        Self {
            value,
            expired: AtomicBool::new(false),
        }
    }

    pub fn set_expired(&self) {
        self.expired.store(true, Ordering::Release);
    }
}

impl<T> Deref for ExpiringValue<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> AsRef<T> for ExpiringValue<T> {
    fn as_ref(&self) -> &T {
        &self.value
    }
}

impl<T> AsMut<T> for ExpiringValue<T> {
    fn as_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

impl<T> Expiring for ExpiringValue<T>
where
    T: Send + Sync,
{
    async fn is_expired(&self) -> bool {
        self.expired.load(Ordering::Acquire)
    }
}

impl<T> From<T> for ExpiringValue<T> {
    fn from(source: T) -> Self {
        ExpiringValue::new(source)
    }
}

/// Cache
pub struct Cache<T, P> {
    provider: P,
    value: RwLock<Option<T>>,
}

impl<T, P> Cache<T, P> {
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            value: RwLock::new(None),
        }
    }

    pub async fn set_expired(&self) {
        let mut value = self.value.write().await;
        *value = None;
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

    pub async fn set_expired(&self) {
        let mut value = self.value.write().await;
        *value = None;
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

#[cfg(test)]
mod tests {
    use std::convert::Infallible;
    use std::sync::atomic::AtomicU8;
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
