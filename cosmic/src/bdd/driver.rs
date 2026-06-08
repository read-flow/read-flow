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

    // -- remotes_manage --
    // No REST surface — `remotes.manage` is client-side bookkeeping in both
    // apps (COSMIC's local DAO, the PWA's IndexedDB), so these only need to
    // support `pwa`/`cosmic` and a scenario carrying just those tags never
    // reaches the `Rest` branch (see `bdd::mod`'s tag-driven filter).

    pub async fn register_remote(&mut self, user: &str, passphrase: &str) {
        match self {
            Self::Rest(_) => panic!(
                "`remotes.manage` has no REST surface — run with BDD_DRIVER=pwa or BDD_DRIVER=cosmic"
            ),
            Self::Cosmic(driver) => driver.register_remote(user, passphrase).await,
        }
    }

    pub async fn remove_registered_remote(&mut self) {
        match self {
            Self::Rest(_) => panic!(
                "`remotes.manage` has no REST surface — run with BDD_DRIVER=pwa or BDD_DRIVER=cosmic"
            ),
            Self::Cosmic(driver) => driver.remove_registered_remote().await,
        }
    }

    pub async fn remote_count(&self) -> usize {
        match self {
            Self::Rest(_) => panic!(
                "`remotes.manage` has no REST surface — run with BDD_DRIVER=pwa or BDD_DRIVER=cosmic"
            ),
            Self::Cosmic(driver) => driver.remote_count().await,
        }
    }

    // -- admin_server_settings --
    // REST and COSMIC manage their own booted backend's settings directly —
    // "viewing" needs no navigation, so there's no dispatcher for that step
    // (see `steps/admin_server_settings.rs`). The PWA leg registers the
    // backend as a remote source first and lives entirely in `pwa/e2e/steps`.

    pub async fn enable_dry_run_and_save(&mut self) {
        match self {
            Self::Rest(driver) => driver.enable_dry_run_and_save().await,
            Self::Cosmic(driver) => driver.enable_dry_run_and_save().await,
        }
    }

    pub async fn dry_run_is_enabled(&self) -> bool {
        match self {
            Self::Rest(driver) => driver.dry_run_is_enabled().await,
            Self::Cosmic(driver) => driver.dry_run_is_enabled().await,
        }
    }

    // -- admin_scan_directories --
    // Same "REST/Cosmic manage their own config directly, no navigation"
    // mapping as `admin_server_settings` — see that section's comment and the
    // feature's doc comment. The PWA leg lives in `pwa/e2e/steps`.

    pub async fn add_scan_directory(&mut self, path: &str) {
        match self {
            Self::Rest(driver) => driver.add_scan_directory(path).await,
            Self::Cosmic(driver) => driver.add_scan_directory(path).await,
        }
    }

    pub async fn scan_directory_is_listed(&self, path: &str) -> bool {
        match self {
            Self::Rest(driver) => driver.scan_directory_is_listed(path).await,
            Self::Cosmic(driver) => driver.scan_directory_is_listed(path).await,
        }
    }

    // -- admin_authorized_users --
    // Same "REST/Cosmic manage their own config directly, no navigation"
    // mapping as `admin_scan_directories` — see that section's comment and the
    // feature's doc comment. The PWA leg lives in `pwa/e2e/steps`.

    pub async fn add_user(&mut self, user_id: &str, password: &str) {
        match self {
            Self::Rest(driver) => driver.add_user(user_id, password).await,
            Self::Cosmic(driver) => driver.add_user(user_id, password).await,
        }
    }

    pub async fn user_is_listed(&self, user_id: &str) -> bool {
        match self {
            Self::Rest(driver) => driver.user_is_listed(user_id).await,
            Self::Cosmic(driver) => driver.user_is_listed(user_id).await,
        }
    }
}

fn env_name() -> &'static str {
    match std::env::var("BDD_DRIVER").as_deref() {
        Ok("cosmic") => "cosmic",
        _ => "rest",
    }
}
