use std::fmt;

use cosmic::Action;
use cosmic::Task;
use cosmic::task;
use provider::r#async::Provider as AsyncProvider;

use crate::state::LoadedState;

pub struct ProvidedState<P, T> {
    pub state: LoadedState<T>,
    state_provider: P,
}

#[derive(Clone)]
pub enum ProvidedStateMessage<T> {
    Load,
    Loaded(T),
    Failed(String),
}

impl<T> fmt::Debug for ProvidedStateMessage<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProvidedStateMessage::Load => write!(f, "ProvidedStateMessage::Load"),
            ProvidedStateMessage::Loaded(_) => write!(f, "ProvidedStateMessage::Loaded with state"),
            ProvidedStateMessage::Failed(error) => {
                write!(f, "ProvidedStateMessage::Failed({})", error)
            }
        }
    }
}

impl<P, T> ProvidedState<P, T> {
    pub fn set_provider(&mut self, provider: P) {
        self.state_provider = provider;
    }

    pub fn provider_mut(&mut self) -> &mut P {
        &mut self.state_provider
    }
}

impl<P, T, E> ProvidedState<P, T>
where
    P: AsyncProvider<T, Error = E> + Clone + 'static,
    T: Send + Sync + 'static,
    E: fmt::Display,
{
    pub fn new(state_provider: P) -> (Self, Task<Action<ProvidedStateMessage<T>>>) {
        (
            Self {
                state: LoadedState::New,
                state_provider,
            },
            task::message(ProvidedStateMessage::Load),
        )
    }

    pub fn update(
        &mut self,
        message: ProvidedStateMessage<T>,
    ) -> Task<Action<ProvidedStateMessage<T>>> {
        tracing::debug!("ProvidedState received: {message:?}");
        match message {
            ProvidedStateMessage::Load => {
                self.state = LoadedState::Loading;
                let state_provider = self.state_provider.clone();
                task::future(async move {
                    match state_provider.provide().await {
                        Ok(state) => ProvidedStateMessage::Loaded(state),
                        Err(error) => ProvidedStateMessage::Failed(format!("{error}")),
                    }
                })
            }
            ProvidedStateMessage::Loaded(state) => {
                self.state = LoadedState::Loaded(state);
                Task::none()
            }
            ProvidedStateMessage::Failed(error) => {
                self.state = LoadedState::Failed(error);
                task::future(async {
                    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                    ProvidedStateMessage::Load
                })
            }
        }
    }
}
