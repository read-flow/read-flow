// SPDX-License-Identifier: AGPL-3.0-or-later

//! Data-access layer over the SQLite schema, split by aggregate:
//!
//! * [`files`] — `files` rows and the high-level scan writer
//! * [`tags`] — `contents` and `content_tags`
//! * [`documents`] — `documents`, metadata, linking/merging, `ApiDocument`
//! * [`reading_state`] — reading progress/status
//! * [`remotes`] — registered remote servers
//! * [`covers`] — stored cover images
//!
//! Everything is re-exported flat (`dao::select_file_by_guid`, ...) so callers
//! are unaffected by the internal layout.
//!
//! ## Connection conventions
//!
//! Single-statement (or logically atomic single-row) operations take
//! `&mut SqliteConnection` and compose into the caller's transaction.
//! Multi-statement operations that must be atomic take `&SqlitePool` and
//! open their own transaction (`merge_documents`, `delete_remote_by_id`,
//! `swap_order_of_remotes`).

mod covers;
mod documents;
mod files;
mod reading_state;
mod remotes;
mod tags;

#[cfg(test)]
mod tests;

use std::io;
use std::sync::Arc;

pub use covers::*;
pub use documents::*;
pub use files::*;
pub use reading_state::*;
pub use remotes::*;
pub use tags::*;

// ─── Error ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Sqlx(#[source] Arc<sqlx::Error>),
    #[error("io error: {0}")]
    IO(#[source] Arc<io::Error>),
}

impl From<sqlx::Error> for Error {
    fn from(value: sqlx::Error) -> Self {
        Self::Sqlx(Arc::new(value))
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::IO(Arc::new(value))
    }
}
