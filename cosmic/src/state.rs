#[derive(Default)]
pub enum LoadedState<T> {
    #[default]
    New,
    Loading,
    Failed(String),
    Loaded(T),
}
