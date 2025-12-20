use std::fmt::Debug;
use std::ops::Deref;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::RwLock;

pub trait Provider<T> {
    type Error;
    fn provide(&self) -> Result<T, Self::Error>;

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
    P: Provider<T, Error = E>,
{
    type Error = E;
    fn provide(&self) -> Result<T, Self::Error> {
        self.as_ref().provide()
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
    P: Provider<T>,
    F: Fn(T) -> R,
{
    type Error = P::Error;
    fn provide(&self) -> Result<R, Self::Error> {
        self.provider.provide().map(&self.transformation)
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
    T: Clone,
{
    type Error = ();
    fn provide(&self) -> Result<T, Self::Error> {
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

impl<T> Expiring for ExpiringValue<T> {
    fn is_expired(&self) -> bool {
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

    pub fn set_expired(&self) {
        let mut value = self.value.write().unwrap();
        *value = None;
    }
}

impl<T, P, E> Provider<T> for Cache<T, P>
where
    P: Provider<T, Error = E>,
    T: Clone,
{
    type Error = E;

    fn provide(&self) -> Result<T, Self::Error> {
        // Try to read the cached value first
        {
            let value = self.value.read().unwrap();
            if let Some(ref cached) = *value {
                return Ok(cached.clone());
            }
        }

        // Value not cached, acquire write lock and populate
        let mut value = self.value.write().unwrap();
        // Double-check after acquiring write lock
        if let Some(ref cached) = *value {
            return Ok(cached.clone());
        }

        let new_value = self.provider.provide()?;
        *value = Some(new_value.clone());
        Ok(new_value)
    }
}

impl<T, P> Expiring for Cache<T, P>
where
    P: Provider<T>,
{
    fn is_expired(&self) -> bool {
        self.value.read().unwrap().is_none()
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

    pub fn set_expired(&self) {
        let mut value = self.value.write().unwrap();
        *value = None;
    }
}

impl<T, P> Expiring for ExpiringItemCache<T, P>
where
    T: Expiring,
{
    fn is_expired(&self) -> bool {
        let value = self.value.read().unwrap();
        match &*value {
            Some(v) => v.is_expired(),
            None => true,
        }
    }
}

impl<T, P, E> Provider<T> for ExpiringItemCache<T, P>
where
    P: Provider<T, Error = E>,
    T: Expiring + Clone,
{
    type Error = E;

    fn provide(&self) -> Result<T, Self::Error> {
        // Try to read the cached value first
        {
            let value = self.value.read().unwrap();
            if let Some(ref cached) = *value {
                if !cached.is_expired() {
                    return Ok(cached.clone());
                }
            }
        }

        // Value not cached or expired, acquire write lock and populate
        let mut value = self.value.write().unwrap();

        // Double-check after acquiring write lock
        if let Some(ref cached) = *value {
            if !cached.is_expired() {
                return Ok(cached.clone());
            }
        }

        let new_value = self.provider.provide()?;
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

        fn provide(&self) -> Result<u8, Self::Error> {
            let result = self.value.load(Ordering::Relaxed) + 1;
            set_expired(false);
            self.value.store(result, Ordering::Release);
            Ok(result)
        }
    }

    impl Expiring for u8 {
        fn is_expired(&self) -> bool {
            get_expired()
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
        provider.set_expired();
        assert_eq!(provider.provide().unwrap(), 3);
        assert_eq!(provider.provide().unwrap(), 3);
    }

    #[test]
    fn test_expiring_cache_provider() {
        let provider = Counter::default().expiring_item_cache();
        assert_eq!(provider.provide().unwrap(), 1);
        assert_eq!(provider.provide().unwrap(), 1);

        set_expired(true);

        assert!(get_expired());

        assert_eq!(provider.provide().unwrap(), 2);
        assert_eq!(provider.provide().unwrap(), 2);
    }
}
