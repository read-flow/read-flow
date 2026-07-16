// SPDX-License-Identifier: AGPL-3.0-or-later

use cosmic::cosmic_config;
use cosmic::cosmic_config::CosmicConfigEntry;
use cosmic::cosmic_config::cosmic_config_derive::CosmicConfigEntry;

#[derive(Debug, Default, Clone, CosmicConfigEntry, Eq, PartialEq)]
#[version = 3]
pub struct Config {
    /// Start the embedded HTTP server automatically when the app launches.
    pub server_start_on_launch: bool,
}
