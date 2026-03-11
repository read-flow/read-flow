// SPDX-License-Identifier: GPL-3.0-or-later

use cosmic::cosmic_config;
use cosmic::cosmic_config::CosmicConfigEntry;
use cosmic::cosmic_config::cosmic_config_derive::CosmicConfigEntry;

#[derive(Debug, Default, Clone, Copy, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum EpubViewerConfig {
    #[default]
    NativeEpub,
    MuPdf,
    ExternalViewer,
}

#[derive(Debug, Default, Clone, CosmicConfigEntry, Eq, PartialEq)]
#[version = 1]
pub struct Config {
    pub epub_viewer: EpubViewerConfig,
}
