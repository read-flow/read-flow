pub mod pipeline;
pub mod scanner;

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;
use itertools::Itertools;
pub use pipeline::ScanProgress;
use provider::r#async::Provider;
pub use scanner::Scanner;
use serde::Deserialize;
use serde::Serialize;

use crate::ApplicationModule;
use crate::ExpandedPath;
use crate::settings::Settings;
use crate::settings::SettingsError;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ScanSettings {
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub auto_tags: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub directories: BTreeMap<ExpandedPath, DirectorySettings>,
    #[serde(default = "default_extensions")]
    pub extensions: Vec<String>,
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
}

fn default_extensions() -> Vec<String> {
    vec!["pdf".into(), "epub".into(), "mobi".into()]
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
    pub async fn scan(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref().canonicalize()?;
        let settings = self.settings().await.scan;
        let pool = self.connection_pool().await;
        let scanner = Scanner::new(settings);
        let mut progress_rx = scanner.scan(path, pool).await;
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
}

#[cfg(test)]
mod tests {
    use assert4rs::Assert;
    use rstest::rstest;

    use super::*;

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
    fn default_extensions_are_pdf_epub_mobi() {
        let settings = ScanSettings::default();
        Assert::that(&settings.extensions).is(&vec![
            "pdf".to_string(),
            "epub".to_string(),
            "mobi".to_string(),
        ]);
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
        Assert::that(&settings.extensions).is(&vec!["pdf".to_string(), "cbz".to_string()]);
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
        let toml = r#"dry_run = false"#;
        let settings: ScanSettings = toml::from_str(toml).unwrap();
        Assert::that(&settings.extensions).is(&vec![
            "pdf".to_string(),
            "epub".to_string(),
            "mobi".to_string(),
        ]);
    }

    #[test]
    fn missing_concurrency_in_toml_uses_default() {
        let toml = r#"dry_run = false"#;
        let settings: ScanSettings = toml::from_str(toml).unwrap();
        Assert::that(settings.concurrency).is(16usize);
    }
}
