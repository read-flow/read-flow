use std::fmt;

pub mod filtered;
pub mod tags;

#[derive(Default, Clone)]
pub enum LoadedState<T> {
    #[default]
    New,
    Loading,
    Failed(String),
    Loaded(T),
}

impl<T> fmt::Debug for LoadedState<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadedState::New => write!(f, "New"),
            LoadedState::Loading => write!(f, "Loading"),
            LoadedState::Failed(_) => write!(f, "Failed"),
            LoadedState::Loaded(_) => write!(f, "Loaded"),
        }
    }
}

impl<T> LoadedState<T> {
    pub fn is_loaded(&self) -> bool {
        matches!(self, LoadedState::Loaded(_))
    }

    // pub fn get_loaded(&self) -> Option<&T> {
    //     if let LoadedState::Loaded(state) = self {
    //         Some(state)
    //     } else {
    //         None
    //     }
    // }

    pub fn unwrap(&self) -> &T {
        if let LoadedState::Loaded(state) = self {
            state
        } else {
            panic!("Invalid state: {self:?}")
        }
    }

    pub fn unwrap_mut(&mut self) -> &mut T {
        if let LoadedState::Loaded(state) = self {
            state
        } else {
            panic!("Invalid state: {self:?}")
        }
    }
}
