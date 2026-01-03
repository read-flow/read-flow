use std::convert::Infallible;
use std::ops::Deref;

use crate::sync::Provider;

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

impl<T> Deref for Value<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
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
    type Error = Infallible;
    fn provide(&self) -> Result<T, Self::Error> {
        Ok(self.0.clone())
    }
}
