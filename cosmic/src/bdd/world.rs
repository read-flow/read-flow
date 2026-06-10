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
    /// File GUID of the most recently seeded document.
    pub current_document_guid: Option<String>,
    /// Fingerprint of the most recently seeded document.
    pub current_document_fingerprint: Option<String>,
    /// Document-record GUID of the most recently seeded document.
    pub current_document_api_guid: Option<String>,
    /// Document-record GUID of the second seeded document (for merge/sort/etc.).
    pub second_document_api_guid: Option<String>,
    /// Temp directory created by an `admin.scan` seed step — kept here so
    /// it outlives the step and is cleaned up when the scenario ends.
    pub _scan_dir: Option<tempfile::TempDir>,
    /// Document count returned by the most recent scan trigger.
    pub _scan_processed: Option<u64>,
    /// Result of the most recent check-missing operation.
    pub _check_missing_result: Option<Vec<String>>,
    /// Search query set by a `When I search for …` step.
    pub search_query: Option<String>,
    /// Status filter set by a `When I filter by reading status …` step.
    pub status_filter: Option<String>,
    /// Tag filter set by a `When I filter by tag …` step.
    pub tag_filter: Option<String>,
    /// Sort direction set by sort steps (true = ascending).
    pub sort_ascending: bool,
}

impl BddWorld {
    async fn init() -> anyhow::Result<Self> {
        Ok(Self {
            driver: Driver::new().await,
            last_check: None,
            current_document_guid: None,
            current_document_fingerprint: None,
            current_document_api_guid: None,
            second_document_api_guid: None,
            _scan_dir: None,
            _scan_processed: None,
            _check_missing_result: None,
            search_query: None,
            status_filter: None,
            tag_filter: None,
            sort_ascending: true,
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
