#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Section {
    #[default]
    Overview,
    Database,
    Client,
    Scan,
    Server,
    Ui,
    OnlineLibrary,
}

impl Section {
    /// All non-overview sections shown as cards on the overview page.
    pub fn all() -> &'static [Section] {
        use Section::*;
        &[Database, Client, Scan, Server, Ui, OnlineLibrary]
    }

    pub fn label(&self) -> &'static str {
        use Section::*;
        match self {
            Overview => "Settings",
            Database => "Database",
            Client => "Client",
            Scan => "Scan",
            Server => "Server",
            Ui => "UI",
            OnlineLibrary => "Online Library",
        }
    }

    pub fn description(&self) -> &'static str {
        use Section::*;
        match self {
            Overview => "",
            Database => "Database file location",
            Client => "Download folder for remote files",
            Scan => "File types, directories, and auto-tag rules",
            Server => "Server download folder and authorized users",
            Ui => "Private mode and private tags",
            OnlineLibrary => "OPDS catalog feeds",
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
    fn all_does_not_contain_overview() {
        assert!(!Section::all().contains(&Section::Overview));
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

    #[test]
    fn descriptions_non_empty() {
        for s in Section::all() {
            assert!(!s.description().is_empty());
        }
    }
}
