// SPDX-License-Identifier: AGPL-3.0-or-later

//! Remote-server registrations (`remotes` table).

use sqlx::SqliteConnection;
use sqlx::SqlitePool;

use super::Error;
use crate::db::models::NewRemote;
use crate::db::models::Remote;

// ─── Remote queries ───────────────────────────────────────────────────────────

pub async fn insert_remote(
    conn: &mut SqliteConnection,
    remote: NewRemote,
) -> Result<Remote, Error> {
    let row = sqlx::query_as::<_, Remote>(
        r#"INSERT INTO remotes (base_url, "order", passphrase, user_id)
           VALUES (?, ?, ?, ?)
           RETURNING id, base_url, "order" AS "order", passphrase, user_id"#,
    )
    .bind(&remote.base_url)
    .bind(remote.order)
    .bind(&remote.passphrase)
    .bind(&remote.user_id)
    .fetch_one(&mut *conn)
    .await?;
    Ok(row)
}

pub async fn select_all_remotes(conn: &mut SqliteConnection) -> Result<Vec<Remote>, Error> {
    let remotes = sqlx::query_as::<_, Remote>(
        r#"SELECT id, base_url, "order", passphrase, user_id FROM remotes ORDER BY "order""#,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(remotes)
}

pub async fn update_remote(
    conn: &mut SqliteConnection,
    id: i32,
    base_url: &str,
    user_id: &str,
    passphrase: &str,
) -> Result<(), Error> {
    sqlx::query("UPDATE remotes SET base_url = ?, user_id = ?, passphrase = ? WHERE id = ?")
        .bind(base_url)
        .bind(user_id)
        .bind(passphrase)
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}

pub async fn delete_remote_by_id(pool: &SqlitePool, id: i32) -> Result<(), Error> {
    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM remotes WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    // Ensure no gaps in the `order` column
    sqlx::query(
        r#"UPDATE remotes
           SET "order" = updated_values.new_order - 1
           FROM (SELECT rowid, ROW_NUMBER() OVER (ORDER BY "order") AS new_order FROM remotes) AS updated_values
           WHERE remotes.rowid = updated_values.rowid"#,
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

pub async fn swap_order_of_remotes(pool: &SqlitePool, a: &Remote, b: &Remote) -> Result<(), Error> {
    let mut tx = pool.begin().await?;

    // Move to temporary negative values to avoid UNIQUE constraint on "order"
    sqlx::query(
        r#"UPDATE remotes SET "order" = CASE id
               WHEN ? THEN ?
               WHEN ? THEN ?
           END
           WHERE id IN (?, ?)"#,
    )
    .bind(a.id)
    .bind(-b.order - 1)
    .bind(b.id)
    .bind(-a.order - 1)
    .bind(a.id)
    .bind(b.id)
    .execute(&mut *tx)
    .await?;

    // Set the final swapped values
    sqlx::query(
        r#"UPDATE remotes SET "order" = CASE id
               WHEN ? THEN ?
               WHEN ? THEN ?
           END
           WHERE id IN (?, ?)"#,
    )
    .bind(a.id)
    .bind(b.order)
    .bind(b.id)
    .bind(a.order)
    .bind(a.id)
    .bind(b.id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}
