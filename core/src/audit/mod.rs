// SPDX-License-Identifier: AGPL-3.0-or-later

//! Structured activity history shared by the database, server, scanner, and clients.

mod event;
mod operation;

pub use event::*;
pub use operation::*;

/// Returns a stable UTC timestamp for audit persistence without coupling the
/// core audit model to a presentation or localization library. Microsecond
/// precision keeps "most recent" ordering effectively deterministic: two
/// back-to-back operations (e.g. a scan followed by a delete over HTTP) land
/// in distinct timestamps, so the listing's `started_at DESC, id DESC` tiebreak
/// only fires for ops created within the same microsecond.
pub fn now_timestamp() -> String {
    let micros = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros();
    micros.to_string()
}
