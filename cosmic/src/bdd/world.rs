//! cucumber `World`: per-scenario state threaded through step definitions.
//! Holds the selected `Driver` plus whatever a scenario needs to remember
//! between its `Given`/`When`/`Then` steps.

use crate::bdd::driver::Driver;

#[derive(cucumber::World)]
#[world(init = Self::init)]
pub struct BddWorld {
    pub driver: Driver,
    /// Set by a `When` step, asserted on by a later `Then` step.
    pub last_check: Option<bool>,
    /// GUID of the most recently seeded document — set by seed steps,
    /// consumed by `When`/`Then` steps that operate on a specific document.
    pub current_document_guid: Option<String>,
}

impl BddWorld {
    async fn init() -> anyhow::Result<Self> {
        Ok(Self {
            driver: Driver::new().await,
            last_check: None,
            current_document_guid: None,
        })
    }
}

impl std::fmt::Debug for BddWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BddWorld")
            .field("driver", &self.driver.name())
            .field("last_check", &self.last_check)
            .finish()
    }
}
