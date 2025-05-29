// SPDX-License-Identifier: GPL-3.0-or-later
use cosmic::Action;

pub trait ActionExt<T> {
    fn map<U, F>(self, f: F) -> Action<U>
    where
        F: FnOnce(T) -> U;
}

impl<T> ActionExt<T> for Action<T> {
    fn map<U, F>(self, f: F) -> Action<U>
    where
        F: FnOnce(T) -> U,
    {
        match self {
            Action::App(msg) => Action::App(f(msg)),
            Action::Cosmic(action) => Action::Cosmic(action),
            Action::DbusActivation(message) => Action::DbusActivation(message),
            Action::None => Action::None,
        }
    }
}
