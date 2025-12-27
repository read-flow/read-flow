use crate::r#async::Expiring;
use crate::r#async::Provider;

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
    P: Provider<T> + Sync,
    F: Fn(T) -> R + Send + Sync,
    T: Send + Sync,
{
    type Error = P::Error;
    async fn provide(&self) -> Result<R, Self::Error> {
        self.provider.provide().await.map(&self.transformation)
    }
}

impl<P, F, T> Expiring for Map<P, F, T>
where
    P: Expiring + Sync,
    F: Send + Sync,
    T: Send + Sync,
{
    async fn is_expired(&self) -> bool {
        self.provider.is_expired().await
    }
}
