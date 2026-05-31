#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SaveState {
    #[default]
    Idle,
    Saving,
    Saved,
    Error(String),
}

impl SaveState {
    pub fn status_text(&self) -> &str {
        match self {
            SaveState::Idle => "",
            SaveState::Saving => "Saving\u{2026}",
            SaveState::Saved => "Saved",
            SaveState::Error(e) => e.as_str(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_idle() {
        assert_eq!(SaveState::default(), SaveState::Idle);
    }

    #[test]
    fn status_text_non_empty_for_active_states() {
        assert!(!SaveState::Saving.status_text().is_empty());
        assert!(!SaveState::Saved.status_text().is_empty());
        assert!(!SaveState::Error("oops".into()).status_text().is_empty());
    }
}
