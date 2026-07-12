use std::sync::Mutex;
use std::sync::mpsc;

/// A simple broadcast channel backed by `std::sync::mpsc`.
///
/// Each call to [`subscribe`] creates a new receiver that will receive every
/// value sent after it subscribed.  Dead receivers are pruned automatically on
/// each [`send`].
///
/// [`subscribe`]: Broadcaster::subscribe
/// [`send`]: Broadcaster::send
pub(crate) struct Broadcaster<T> {
    senders: Mutex<Vec<mpsc::Sender<T>>>,
}

impl<T> Broadcaster<T> {
    pub(crate) fn new() -> Self {
        Self {
            senders: Mutex::new(Vec::new()),
        }
    }

    /// Register a new subscriber and return its receiving end.
    pub(crate) fn subscribe(&self) -> mpsc::Receiver<T> {
        let (tx, rx) = mpsc::channel();
        self.senders.lock().unwrap().push(tx);
        rx
    }
}

impl<T: Clone> Broadcaster<T> {
    /// Send `value` to all current subscribers, dropping any that have hung up.
    pub(crate) fn send(&self, value: T) {
        let mut senders = self.senders.lock().unwrap();
        senders.retain(|tx| tx.send(value.clone()).is_ok());
    }
}
