pub mod file_system_visitor;
pub mod modules;
use std::{path::Path, sync::Arc};

use anyhow::Result;
use indexmap::IndexMap;
use itertools::Itertools;
use serde::Deserialize;

use crate::{db::ConnectionPool, ApplicationModule, ExpandedPath};

pub use file_system_visitor::{Error, FileSystemVisitor};
use modules::{file_extension_finder::FileExtensionFinder, scm_project_finder::ScmProjectFinder};

#[derive(Debug, Clone, Deserialize)]
pub struct ScanSettings {
    pub dry_run: bool,
    pub auto_tags: IndexMap<String, Vec<String>>,
    pub directories: IndexMap<ExpandedPath, DirectorySettings>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "action")]
pub enum DirectorySettings {
    Ignore { inherit: bool },
    Scan { tags: Vec<String>, inherit: bool },
}

impl DirectorySettings {
    fn empty_scan() -> Self {
        Self::Scan {
            tags: Default::default(),
            inherit: Default::default(),
        }
    }

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

    fn inherit(&self) -> bool {
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
}

pub fn create_visitor(
    connection_pool: ConnectionPool,
    scan_settings: ScanSettings,
) -> FileSystemVisitor {
    let scan_settings = Arc::new(scan_settings);
    FileSystemVisitor::new(
        vec![
            Box::new(ScmProjectFinder::new(
                ".git".into(),
                connection_pool.clone(),
            )),
            Box::new(ScmProjectFinder::new(".hg".into(), connection_pool.clone())),
        ],
        vec![
            Box::new(FileExtensionFinder::new(
                "pdf".into(),
                connection_pool.clone(),
                scan_settings.clone(),
            )),
            Box::new(FileExtensionFinder::new(
                "epub".into(),
                connection_pool.clone(),
                scan_settings.clone(),
            )),
            Box::new(FileExtensionFinder::new(
                "mobi".into(),
                connection_pool,
                scan_settings,
            )),
        ],
    )
}

impl ApplicationModule {
    pub fn scan(self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref().canonicalize()?;
        self.visitor().visit(&path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use assert4rs::{Assert, AssertEquals};
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
}
