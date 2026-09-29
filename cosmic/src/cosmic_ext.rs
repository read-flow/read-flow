// SPDX-License-Identifier: AGPL-3.0-or-later
use cosmic::Action;

pub trait ActionExt<T> {
    fn map_into<U>(self) -> Action<U>
    where
        T: Into<U> + 'static,
        U: 'static;
}

impl<T> ActionExt<T> for Action<T> {
    fn map_into<U>(self) -> Action<U>
    where
        T: Into<U> + 'static,
        U: 'static,
    {
        // `Action::map` also maps the messages inside `Action::Surface`.
        self.map(Into::into)
    }
}
