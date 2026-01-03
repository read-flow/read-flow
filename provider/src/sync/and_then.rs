use crate::sync::Expiring;
use crate::sync::Provider;

pub struct AndThen<P, F, T> {
    provider: P,
    transformation: F,
    _marker: std::marker::PhantomData<T>,
}

impl<P, F, T> AndThen<P, F, T> {
    pub fn new(provider: P, transformation: F) -> Self {
        Self {
            provider,
            transformation,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T, P, R, F> Provider<R> for AndThen<P, F, T>
where
    P: Provider<T>,
    F: Fn(T) -> Result<R, P::Error>,
{
    type Error = P::Error;
    fn provide(&self) -> Result<R, P::Error> {
        self.provider.provide().and_then(&self.transformation)
    }
}

impl<P, F, T> Expiring for AndThen<P, F, T>
where
    P: Expiring,
{
    fn is_expired(&self) -> bool {
        self.provider.is_expired()
    }
}
