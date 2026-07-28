// SPDX-License-Identifier: AGPL-3.0-or-later

//! Cover images (`covers` table).

use sqlx::SqliteConnection;

use super::Error;

// ─── Cover queries ────────────────────────────────────────────────────────────

/// Return the set of all fingerprints that have a stored cover image.
pub async fn select_fingerprints_with_covers(
    conn: &mut SqliteConnection,
) -> Result<std::collections::HashSet<String>, Error> {
    let rows: Vec<String> = sqlx::query_scalar("SELECT fingerprint FROM covers")
        .fetch_all(&mut *conn)
        .await?;
    Ok(rows.into_iter().collect())
}

pub async fn upsert_cover(
    conn: &mut SqliteConnection,
    fingerprint: &str,
    data: &[u8],
    mime: &str,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO covers (fingerprint, data, mime) VALUES (?, ?, ?) \
         ON CONFLICT(fingerprint) DO UPDATE SET data = excluded.data, mime = excluded.mime",
    )
    .bind(fingerprint)
    .bind(data)
    .bind(mime)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn get_cover(
    conn: &mut SqliteConnection,
    fingerprint: &str,
) -> Result<Option<(Vec<u8>, String)>, Error> {
    let row = sqlx::query_as::<_, (Vec<u8>, String)>(
        "SELECT data, mime FROM covers WHERE fingerprint = ?",
    )
    .bind(fingerprint)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(row)
}

pub async fn cover_exists(conn: &mut SqliteConnection, fingerprint: &str) -> Result<bool, Error> {
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM covers WHERE fingerprint = ?)")
            .bind(fingerprint)
            .fetch_one(&mut *conn)
            .await?;
    Ok(exists)
}
