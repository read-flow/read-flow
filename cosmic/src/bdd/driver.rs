//! Picks which surface a scenario run exercises. Step definitions dispatch
//! through this enum rather than hard-coding a driver, so the same Gherkin
//! steps run against either surface depending on `BDD_DRIVER`.

use crate::bdd::cosmic_driver::CosmicDriver;
use crate::bdd::rest_driver::RestDriver;

pub enum Driver {
    Rest(RestDriver),
    Cosmic(CosmicDriver),
}

impl Driver {
    /// Selects the driver via `BDD_DRIVER=rest|cosmic` (default `rest`).
    /// Must be paired with a matching cucumber tag filter — see `bdd::mod`.
    pub async fn new() -> Self {
        match env_name() {
            "cosmic" => Self::Cosmic(CosmicDriver::new().await),
            _ => Self::Rest(RestDriver::new().await),
        }
    }

    /// Same selection as [`Self::new`], without booting anything — used to
    /// derive the scenario tag filter (`@rest`/`@cosmic`) up front.
    pub fn env_name() -> &'static str {
        env_name()
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Rest(_) => "rest",
            Self::Cosmic(_) => "cosmic",
        }
    }

    /// The booted backend's URL — both drivers boot a real `TestServer`
    /// (REST hits it directly; COSMIC needs it as something for `Remote`s
    /// to actually reach over HTTP).
    pub fn base_url(&self) -> &str {
        match self {
            Self::Rest(driver) => driver.base_url(),
            Self::Cosmic(driver) => driver.base_url(),
        }
    }

    // -- _smoke_rest --

    pub async fn status_is_healthy(&self) -> bool {
        match self {
            Self::Rest(driver) => driver.status_is_healthy().await,
            Self::Cosmic(_) => panic!(
                "this step only supports the `rest` driver (run with BDD_DRIVER=rest and a matching @rest tag filter)"
            ),
        }
    }

    // -- remotes_status --

    /// "Add that server as a remote source" has no single natural shape
    /// across surfaces — REST has no "add remote" concept (the When step
    /// maps to "call /status with these creds" directly), while COSMIC
    /// inserts a `Remote` row pointing at the driver's own booted backend
    /// and drives `CheckSourceStatus`. Each driver returns the same
    /// observable: is the source reported as reachable?
    pub async fn add_remote_and_check_status(&mut self, user: &str, passphrase: &str) -> bool {
        match self {
            Self::Rest(driver) => driver.status_with(user, passphrase).await,
            Self::Cosmic(driver) => {
                let base_url = driver.base_url().to_string();
                let remote = driver.insert_remote(&base_url, user, passphrase).await;
                driver.check_source_status(&remote).await
            }
        }
    }
}

fn env_name() -> &'static str {
    match std::env::var("BDD_DRIVER").as_deref() {
        Ok("cosmic") => "cosmic",
        _ => "rest",
    }
}
