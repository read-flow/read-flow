// SPDX-License-Identifier: AGPL-3.0-or-later

//! Reading progress and status (`reading_state` table).
//!
//! Rows are keyed by `(user_id, fingerprint)`: every authorized user has
//! independent progress and status. Local (non-authenticated) access — the
//! desktop GUI and CLI operating directly on the database — uses the reserved
//! [`crate::db::LOCAL_USER_ID`].

use std::sync::Arc;

use sqlx::SqliteConnection;

use super::Error;
use crate::db::models::ReadingState;

// ─── Reading state queries ────────────────────────────────────────────────────

pub async fn get_reading_state(
    conn: &mut SqliteConnection,
    user_id: &str,
    fingerprint: &str,
) -> Result<Option<ReadingState>, Error> {
    sqlx::query_as::<_, ReadingState>(
        "SELECT fingerprint, status, position, percentage, last_updated, status_updated_at \
         FROM reading_state WHERE user_id = ? AND fingerprint = ?",
    )
    .bind(user_id)
    .bind(fingerprint)
    .fetch_optional(&mut *conn)
    .await
    .map_err(Into::into)
}

/// Upsert reading state with server-side auto-transitions:
/// - Unread (0) + percentage > 0.01  → Reading (1)
/// - Reading (1) + percentage ≥ 0.99 → Read (2)
/// - Read (2): no auto-downgrade
///
/// Returns the resulting state (after any transitions).
pub async fn upsert_reading_state(
    conn: &mut SqliteConnection,
    user_id: &str,
    state: ReadingState,
) -> Result<ReadingState, Error> {
    // For the INSERT (new row) path, compute initial status from percentage.
    let initial_status: i32 = if state.percentage > 0.01 { 1 } else { 0 };
    let initial_status_updated_at = if initial_status > 0 {
        state.last_updated.clone()
    } else {
        state.status_updated_at.clone()
    };

    sqlx::query(
        r#"INSERT INTO reading_state
               (user_id, fingerprint, status, position, percentage, last_updated, status_updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?)
           ON CONFLICT(user_id, fingerprint) DO UPDATE
           SET position          = excluded.position,
               percentage        = excluded.percentage,
               last_updated      = excluded.last_updated,
               status            = CASE
                   WHEN reading_state.status = 0 AND excluded.percentage > 0.01  THEN 1
                   WHEN reading_state.status = 1 AND excluded.percentage >= 0.99 THEN 2
                   ELSE reading_state.status
               END,
               status_updated_at = CASE
                   WHEN (reading_state.status = 0 AND excluded.percentage > 0.01)
                     OR (reading_state.status = 1 AND excluded.percentage >= 0.99)
                     THEN excluded.last_updated
                   ELSE reading_state.status_updated_at
               END
           WHERE excluded.last_updated > reading_state.last_updated"#,
    )
    .bind(user_id)
    .bind(&state.fingerprint)
    .bind(initial_status)
    .bind(&state.position)
    .bind(state.percentage)
    .bind(&state.last_updated)
    .bind(&initial_status_updated_at)
    .execute(&mut *conn)
    .await?;

    get_reading_state(conn, user_id, &state.fingerprint)
        .await?
        .ok_or_else(|| Error::Sqlx(Arc::new(sqlx::Error::RowNotFound)))
}

/// Manually override the reading status. Bypasses auto-transition rules.
/// Creates a reading_state row if none exists.
pub async fn update_reading_status_only(
    conn: &mut SqliteConnection,
    user_id: &str,
    fingerprint: &str,
    status: i32,
) -> Result<(), Error> {
    sqlx::query(
        r#"INSERT INTO reading_state (user_id, fingerprint, status, status_updated_at, last_updated)
           VALUES (?, ?, ?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
           ON CONFLICT(user_id, fingerprint) DO UPDATE
           SET status            = excluded.status,
               status_updated_at = excluded.status_updated_at"#,
    )
    .bind(user_id)
    .bind(fingerprint)
    .bind(status)
    .execute(&mut *conn)
    .await?;
    Ok(())
}
