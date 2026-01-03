use std::ops::Deref;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use crate::sync::Expiring;
use crate::sync::Provider;

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

pub struct Expired;

impl<T> Provider<T> for ExpiringValue<T>
where
    T: Clone,
{
    type Error = Expired;

    fn provide(&self) -> Result<T, Self::Error> {
        if self.is_expired() {
            Err(Expired)
        } else {
            Ok(self.value.clone())
        }
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
