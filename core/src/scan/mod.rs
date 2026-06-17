pub mod cover;
pub mod metadata;
pub mod pipeline;
pub mod scanner;

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;
use itertools::Itertools;
pub use pipeline::ScanProgress;
use provider::r#async::Provider;
pub use scanner::Scanner;

/// Aggregated outcome of one or more scans.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanSummary {
    pub discovered: u64,
    pub processed: u64,
    pub errors: u64,
}

impl ScanSummary {
    /// Fold a progress event into the running summary (only `Completed` counts).
    pub fn add_event(&mut self, event: &ScanProgress) {
        if let ScanProgress::Completed {
            discovered,
            processed,
            errors,
        } = event
        {
            self.discovered += discovered;
            self.processed += processed;
            self.errors += errors;
        }
    }
}
use serde::Deserialize;
use serde::Serialize;
use tokio::sync::mpsc;

use crate::ApplicationModule;
use crate::ExpandedPath;
use crate::settings::Settings;
use crate::settings::SettingsError;

/// A document file type supported by the MuPDF renderer, plus a catch-all `Other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DocumentType {
    Pdf,
    Epub,
    Mobi,
    Azw,
    Azw3,
    Fb2,
    Cbz,
    Cbt,
    Xps,
    Oxps,
    Docx,
    Xlsx,
    Pptx,
    Hwpx,
    /// Any file type not natively supported — opened via the external viewer.
    Other,
}

impl DocumentType {
    /// The file extension string for this document type (e.g. `"pdf"`).
    pub fn as_str(&self) -> &'static str {
        use DocumentType::*;
        match self {
            Pdf => "pdf",
            Epub => "epub",
            Mobi => "mobi",
            Azw => "azw",
            Azw3 => "azw3",
            Fb2 => "fb2",
            Cbz => "cbz",
            Cbt => "cbt",
            Xps => "xps",
            Oxps => "oxps",
            Docx => "docx",
            Xlsx => "xlsx",
            Pptx => "pptx",
            Hwpx => "hwpx",
            Other => "other",
        }
    }

    /// A short human-readable label for use in UI settings.
    pub fn label(&self) -> &'static str {
        use DocumentType::*;
        match self {
            Pdf => "PDF",
            Epub => "EPUB",
            Mobi => "MOBI / Kindle",
            Azw => "Kindle AZW",
            Azw3 => "Kindle AZW3",
            Fb2 => "FictionBook",
            Cbz => "Comic Book Archive (ZIP)",
            Cbt => "Comic Book Archive (TAR)",
            Xps => "XPS",
            Oxps => "OpenXPS",
            Docx => "Word Document",
            Xlsx => "Excel Spreadsheet",
            Pptx => "PowerPoint Presentation",
            Hwpx => "Hangul Word Processor",
            Other => "Other",
        }
    }

    /// Icon name for this document type (freedesktop icon naming).
    pub fn get_file_type_icon(&self) -> &'static str {
        use DocumentType::*;
        match self {
            Pdf => "application-pdf",
            Epub => "application-epub+zip",
            Mobi | Azw | Azw3 => "application-x-mobipocket-ebook",
            Fb2 => "text-x-generic",
            Cbz | Cbt => "application-zip",
            Xps | Oxps => "text-x-generic",
            Docx | Hwpx => "x-office-document",
            Xlsx => "x-office-spreadsheet",
            Pptx => "x-office-presentation",
            Other => "text-x-generic",
        }
    }

    /// All named document types in their canonical order. Does not include `Other`.
    pub fn all() -> &'static [DocumentType] {
        use DocumentType::*;
        &[
            Pdf, Epub, Mobi, Azw, Azw3, Fb2, Cbz, Cbt, Xps, Oxps, Docx, Xlsx, Pptx, Hwpx,
        ]
    }
}

impl std::fmt::Display for DocumentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for DocumentType {
    /// Parsing is infallible: unknown strings map to `Other`.
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use DocumentType::*;
        Ok(match s.to_ascii_lowercase().as_str() {
            "pdf" => Pdf,
            "epub" => Epub,
            "mobi" => Mobi,
            "azw" => Azw,
            "azw3" => Azw3,
            "fb2" => Fb2,
            "cbz" => Cbz,
            "cbt" => Cbt,
            "xps" => Xps,
            "oxps" => Oxps,
            "docx" => Docx,
            "xlsx" => Xlsx,
            "pptx" => Pptx,
            "hwpx" => Hwpx,
            _ => Other,
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ScanSettings {
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub auto_tags: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub directories: BTreeMap<ExpandedPath, DirectorySettings>,
    #[serde(default = "default_extensions")]
    pub extensions: Vec<DocumentType>,
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
}

fn default_extensions() -> Vec<DocumentType> {
    use DocumentType::*;
    vec![Pdf, Epub, Mobi]
}

fn default_concurrency() -> usize {
    16
}

impl Default for ScanSettings {
    fn default() -> Self {
        Self {
            dry_run: false,
            auto_tags: Default::default(),
            directories: Default::default(),
            extensions: default_extensions(),
            concurrency: default_concurrency(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "action")]
pub enum DirectorySettings {
    Ignore {
        inherit: bool,
    },
    Scan {
        #[serde(default)]
        tags: Vec<String>,
        inherit: bool,
    },
}

impl DirectorySettings {
    fn merge(self, other: Self) -> Self {
        use DirectorySettings::*;
        if !other.inherit() {
            return other;
        }
        match (self, other) {
            (Ignore { .. }, other) | (other, Ignore { .. }) => other,
            (
                Scan {
                    tags: tags1,
                    inherit,
                },
                Scan { tags: tags2, .. },
            ) => {
                let mut tags = tags1;
                tags.extend(tags2);
                Scan { tags, inherit }
            }
        }
    }

    pub fn inherit(&self) -> bool {
        use DirectorySettings::*;
        match self {
            Ignore { inherit, .. } | Scan { inherit, .. } => *inherit,
        }
    }
}

impl ScanSettings {
    pub fn directory_settings_of(&self, path: impl AsRef<Path>) -> Option<DirectorySettings> {
        let path = path.as_ref();
        self.directories
            .iter()
            .filter(|(dir, _settings)| path.starts_with(dir))
            .sorted_by_key(|(dir, _settings)| *dir)
            .map(|(_key, value)| value)
            .cloned()
            .reduce(|acc, item| acc.merge(item))
    }

    pub fn set_dry_run(&mut self, dry_run: bool) {
        self.dry_run = dry_run;
    }

    pub fn merge_dry_run(&mut self, dry_run: bool) {
        self.dry_run |= dry_run;
    }
}

impl<P> ApplicationModule<P>
where
    P: Provider<Settings, Error = SettingsError> + Send + Sync,
{
    /// Start a scan and return a receiver for progress events.
    /// The caller is responsible for consuming all events from the receiver.
    pub async fn start_scan(&self, path: impl AsRef<Path>) -> Result<mpsc::Receiver<ScanProgress>> {
        let path = path.as_ref().canonicalize()?;
        let settings = self.settings().await.scan;
        let pool = self.connection_pool().await;
        let scanner = Scanner::new(settings);
        Ok(scanner.scan(path, pool).await)
    }

    pub async fn scan(&self, path: impl AsRef<Path>) -> Result<()> {
        let mut progress_rx = self.start_scan(path).await?;
        while let Some(event) = progress_rx.recv().await {
            if let ScanProgress::Completed {
                discovered,
                processed,
                errors,
            } = event
            {
                tracing::info!(
                    "scan complete: {discovered} discovered, {processed} processed, {errors} errors"
                );
            }
        }
        Ok(())
    }

    /// Scan every configured `Scan` directory and return an aggregated summary.
    /// Used by the REST `POST /scan` endpoint.
    pub async fn scan_configured(&self) -> Result<ScanSummary> {
        let directories: Vec<ExpandedPath> = self
            .settings()
            .await
            .scan
            .directories
            .iter()
            .filter(|(_, settings)| matches!(settings, DirectorySettings::Scan { .. }))
            .map(|(dir, _)| dir.clone())
            .collect();

        let mut summary = ScanSummary::default();
        for dir in directories {
            let mut progress_rx = self.start_scan(&dir).await?;
            while let Some(event) = progress_rx.recv().await {
                summary.add_event(&event);
            }
        }
        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use assert4rs::Assert;
    use rstest::rstest;

    use super::*;

    #[test]
    fn scan_summary_aggregates_completed_events() {
        let mut summary = ScanSummary::default();
        // Non-Completed events are ignored.
        summary.add_event(&ScanProgress::FileDiscovered);
        summary.add_event(&ScanProgress::Completed {
            discovered: 3,
            processed: 2,
            errors: 1,
        });
        summary.add_event(&ScanProgress::Completed {
            discovered: 4,
            processed: 4,
            errors: 0,
        });
        assert_eq!(
            summary,
            ScanSummary {
                discovered: 7,
                processed: 6,
                errors: 1,
            }
        );
    }

    fn test_settings(inherit: bool) -> ScanSettings {
        let auto_tags = Default::default();
        let directories = vec![
            (
                "/tmp/ignore".parse().unwrap(),
                DirectorySettings::Ignore { inherit },
            ),
            (
                "/tmp".parse().unwrap(),
                DirectorySettings::Scan {
                    tags: vec!["a".to_string()],
                    inherit: true, // To test that this is not used the other way around
                },
            ),
            (
                "/tmp/ignore/b".parse().unwrap(),
                DirectorySettings::Scan {
                    tags: vec!["b".to_string()],
                    inherit,
                },
            ),
            (
                "/tmp/other".parse().unwrap(),
                DirectorySettings::Scan {
                    tags: vec!["c".to_string()],
                    inherit,
                },
            ),
        ]
        .into_iter()
        .collect();

        ScanSettings {
            dry_run: true,
            auto_tags,
            directories,
            ..ScanSettings::default()
        }
    }

    #[rstest]
    #[case(test_settings(true), "/home/x.pdf", None)]
    #[case(test_settings(true), "/tmp/x.pdf", Some(DirectorySettings::Scan { tags: vec!["a".to_string()], inherit: true }))]
    #[case(test_settings(true), "/tmp/ignore/x.pdf", Some(DirectorySettings::Scan { tags: vec!["a".to_string()], inherit: true }))]
    #[case(test_settings(true), "/tmp/ignore/b/x.pdf", Some(DirectorySettings::Scan { tags: vec!["a".to_string(), "b".to_string()], inherit: true }))]
    #[case(test_settings(true), "/tmp/other/x.pdf", Some(DirectorySettings::Scan { tags: vec!["a".to_string(), "c".to_string()], inherit: true }))]
    #[case(test_settings(false), "/home/x.pdf", None)]
    #[case(test_settings(false), "/tmp/x.pdf", Some(DirectorySettings::Scan { tags: vec!["a".to_string()], inherit: true }))]
    #[case(test_settings(false), "/tmp/ignore/x.pdf", Some(DirectorySettings::Ignore { inherit: false }))]
    #[case(test_settings(false), "/tmp/ignore/b/x.pdf", Some(DirectorySettings::Scan { tags: vec!["b".to_string()], inherit: false }))]
    #[case(test_settings(false), "/tmp/other/x.pdf", Some(DirectorySettings::Scan { tags: vec!["c".to_string()], inherit: false }))]
    #[case(test_settings(true), "/home", None)]
    #[case(test_settings(true), "/tmp", Some(DirectorySettings::Scan { tags: vec!["a".to_string()], inherit: true }))]
    #[case(test_settings(true), "/tmp/ignore", Some(DirectorySettings::Scan { tags: vec!["a".to_string()], inherit: true }))]
    #[case(test_settings(true), "/tmp/ignore/b", Some(DirectorySettings::Scan { tags: vec!["a".to_string(), "b".to_string()], inherit: true }))]
    #[case(test_settings(true), "/tmp/other", Some(DirectorySettings::Scan { tags: vec!["a".to_string(), "c".to_string()], inherit: true }))]
    #[case(test_settings(false), "/home", None)]
    #[case(test_settings(false), "/tmp", Some(DirectorySettings::Scan { tags: vec!["a".to_string()], inherit: true }))]
    #[case(test_settings(false), "/tmp/ignore", Some(DirectorySettings::Ignore { inherit: false }))]
    #[case(test_settings(false), "/tmp/ignore/b", Some(DirectorySettings::Scan { tags: vec!["b".to_string()], inherit: false }))]
    #[case(test_settings(false), "/tmp/other", Some(DirectorySettings::Scan { tags: vec!["c".to_string()], inherit: false }))]
    fn test_scan_settings(
        #[case] settings: ScanSettings,
        #[case] path: &str,
        #[case] expected: Option<DirectorySettings>,
    ) {
        let actual = settings.directory_settings_of(path);
        Assert::that(actual).is(expected);
    }

    #[test]
    fn document_type_azw_round_trips() {
        assert_eq!("azw".parse::<DocumentType>().unwrap(), DocumentType::Azw);
        assert_eq!(DocumentType::Azw.as_str(), "azw");
        assert_eq!(DocumentType::Azw.label(), "Kindle AZW");
    }

    #[test]
    fn document_type_azw3_round_trips() {
        assert_eq!("azw3".parse::<DocumentType>().unwrap(), DocumentType::Azw3);
        assert_eq!(DocumentType::Azw3.as_str(), "azw3");
        assert_eq!(DocumentType::Azw3.label(), "Kindle AZW3");
    }

    #[test]
    fn default_extensions_are_pdf_epub_mobi() {
        use DocumentType::*;
        let settings = ScanSettings::default();
        Assert::that(&settings.extensions).is(&vec![Pdf, Epub, Mobi]);
    }

    #[test]
    fn default_concurrency_is_16() {
        let settings = ScanSettings::default();
        Assert::that(settings.concurrency).is(16usize);
    }

    #[test]
    fn extensions_deserialized_from_toml() {
        let toml = r#"
            dry_run = false
            extensions = ["pdf", "cbz"]
        "#;
        let settings: ScanSettings = toml::from_str(toml).unwrap();
        Assert::that(&settings.extensions).is(&vec![DocumentType::Pdf, DocumentType::Cbz]);
    }

    #[test]
    fn concurrency_deserialized_from_toml() {
        let toml = r#"
            dry_run = false
            concurrency = 8
        "#;
        let settings: ScanSettings = toml::from_str(toml).unwrap();
        Assert::that(settings.concurrency).is(8usize);
    }

    #[test]
    fn missing_extensions_in_toml_uses_default() {
        use DocumentType::*;
        let toml = r#"dry_run = false"#;
        let settings: ScanSettings = toml::from_str(toml).unwrap();
        Assert::that(&settings.extensions).is(&vec![Pdf, Epub, Mobi]);
    }

    #[test]
    fn missing_concurrency_in_toml_uses_default() {
        let toml = r#"dry_run = false"#;
        let settings: ScanSettings = toml::from_str(toml).unwrap();
        Assert::that(settings.concurrency).is(16usize);
    }
}
