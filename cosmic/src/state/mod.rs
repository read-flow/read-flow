pub mod filtered;
pub mod tags;

#[derive(Default)]
pub enum LoadedState<T> {
    #[default]
    New,
    Loading,
    Failed(String),
    Loaded(T),
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
            panic!("Invalid state")
        }
    }

    pub fn unwrap_mut(&mut self) -> &mut T {
        if let LoadedState::Loaded(state) = self {
            state
        } else {
            panic!("Invalid state")
        }
    }
}
