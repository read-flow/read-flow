// SPDX-License-Identifier: GPL-3.0-or-later

use cosmic::cosmic_config;
use cosmic::cosmic_config::CosmicConfigEntry;
use cosmic::cosmic_config::cosmic_config_derive::CosmicConfigEntry;

#[derive(Debug, Default, Clone, CosmicConfigEntry, Eq, PartialEq)]
#[version = 1]
pub struct Config {
    demo: String,
}
