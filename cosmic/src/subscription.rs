use std::sync::Arc;

use cosmic::iced::Subscription;
use futures::SinkExt;
use futures::channel::mpsc;
use futures::stream::BoxStream;
use rand::TryRng;
use rand::rngs::SysRng;
use tokio::sync::Mutex;
use tokio::sync::broadcast;

use crate::ApplicationModule;

pub struct SubscriberState<E, F> {
    id: u64,
    receiver: Arc<Mutex<broadcast::Receiver<E>>>,
    f: Arc<F>,
}

impl<E, F> std::hash::Hash for SubscriberState<E, F> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<E, F, M> SubscriberState<E, F>
where
    F: Fn() -> M + Send + Sync + 'static,
    M: Send + 'static,
    E: Clone + Send + 'static,
{
    pub fn new(receiver: broadcast::Receiver<E>, f: F) -> Self {
        let id = SysRng.try_next_u64().unwrap();
        Self {
            id,
            receiver: Arc::new(Mutex::new(receiver)),
            f: Arc::new(f),
        }
    }

    pub fn run(&self) -> BoxStream<'static, M> {
        let receiver = self.receiver.clone();
        let f = self.f.clone();
        Box::pin(cosmic::iced::stream::channel(
            4,
            async move |mut sender: mpsc::Sender<M>| {
                loop {
                    match receiver.lock().await.recv().await {
                        Ok(_) => {
                            if sender.send((f)()).await.is_err() {
                                // Channel closed, stop the subscription
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            // Sender dropped, stop the subscription
                            break;
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            // Missed some messages, but continue listening
                            // Still send a notification since data has changed
                            if sender.send((f)()).await.is_err() {
                                break;
                            }
                        }
                    }
                }
            },
        ))
    }
}

/// Emits `f()` whenever `application_module`'s settings/document data changes.
pub fn settings_invalidation_subscription<M, F>(
    application_module: Arc<ApplicationModule>,
    f: F,
) -> Subscription<M>
where
    M: Send + 'static,
    F: Fn() -> M + Send + 'static,
    F: Send + Sync + 'static,
{
    let receiver = application_module.subscribe();

    Subscription::run_with(SubscriberState::new(receiver, f), SubscriberState::run)
}
