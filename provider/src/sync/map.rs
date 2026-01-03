use crate::sync::Expiring;
use crate::sync::Provider;

pub struct Map<P, F, T> {
    provider: P,
    transformation: F,
    _marker: std::marker::PhantomData<T>,
}

impl<P, F, T> Map<P, F, T> {
    pub fn new(provider: P, transformation: F) -> Self {
        Self {
            provider,
            transformation,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T, P, R, F> Provider<R> for Map<P, F, T>
where
    P: Provider<T>,
    F: Fn(T) -> R,
{
    type Error = P::Error;
    fn provide(&self) -> Result<R, Self::Error> {
        self.provider.provide().map(&self.transformation)
    }
}

impl<P, F, T> Expiring for Map<P, F, T>
where
    P: Expiring,
{
    fn is_expired(&self) -> bool {
        self.provider.is_expired()
    }
}
