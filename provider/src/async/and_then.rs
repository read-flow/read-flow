use crate::r#async::Expiring;
use crate::r#async::Provider;

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

impl<P, F, T, R> Provider<R> for AndThen<P, F, T>
where
    P: Provider<T> + Sync,
    F: Fn(T) -> Result<R, P::Error> + Send + Sync,
    T: Send + Sync,
{
    type Error = P::Error;
    async fn provide(&self) -> Result<R, P::Error> {
        self.provider.provide().await.and_then(&self.transformation)
    }
}

impl<P, F, T> Expiring for AndThen<P, F, T>
where
    P: Expiring + Sync,
    F: Send + Sync,
    T: Send + Sync,
{
    async fn is_expired(&self) -> bool {
        self.provider.is_expired().await
    }
}
