use std::sync::RwLock;

use crate::sync::Expiring;
use crate::sync::HasSetExpired;
use crate::sync::Provider;

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

// Implement HasSetExpired for ExpiringItemCache
impl<T, P> HasSetExpired for ExpiringItemCache<T, P> {
    fn set_expired(&self) {
        let mut value = self.value.write().unwrap();
        *value = None;
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
