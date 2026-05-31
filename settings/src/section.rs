#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Section {
    #[default]
    Database,
    Client,
    Scan,
    Server,
    Ui,
    OnlineLibrary,
}

impl Section {
    pub fn all() -> &'static [Section] {
        use Section::*;
        &[Database, Client, Scan, Server, Ui, OnlineLibrary]
    }

    pub fn label(&self) -> &'static str {
        use Section::*;
        match self {
            Database => "Database",
            Client => "Client",
            Scan => "Scan",
            Server => "Server",
            Ui => "UI",
            OnlineLibrary => "Online Library",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_returns_six_sections() {
        assert_eq!(Section::all().len(), 6);
    }

    #[test]
    fn all_variants_covered() {
        use Section::*;
        let all = Section::all();
        assert!(all.contains(&Database));
        assert!(all.contains(&Client));
        assert!(all.contains(&Scan));
        assert!(all.contains(&Server));
        assert!(all.contains(&Ui));
        assert!(all.contains(&OnlineLibrary));
    }

    #[test]
    fn labels_non_empty() {
        for s in Section::all() {
            assert!(!s.label().is_empty());
        }
    }
}
