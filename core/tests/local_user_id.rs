//! Regression tests for `server.local_user_id`: configuring COSMIC's local
//! database access to act as a real authorized user, so reading progress
//! recorded locally (via `DbClient`, no auth) is visible to that same user's
//! REST/PWA sessions, and vice versa.
#![cfg(feature = "server")]

use std::path::PathBuf;

use assert4rs::Assert;
use axum::Router;
use axum::body::Body;
use axum::body::to_bytes;
use axum::http::Request;
use axum::http::StatusCode;
use axum::http::header;
use base64::Engine;
use read_flow_core::ApplicationModule;
use read_flow_core::api::FileDataSource;
use read_flow_core::api::ReadingState;
use read_flow_core::server;
use read_flow_core::settings::HashedPassword;
use tower::ServiceExt;

/// Build a router and config with one owner user `alice`, seeded with one
/// file. `configure_local_user_id` controls whether `server.local_user_id`
/// is set to `alice` (the case under test) or left unset (today's default,
/// purely-local behaviour, used as a control).
async fn test_setup(
    configure_local_user_id: bool,
) -> (Router, tempfile::TempDir, PathBuf, String, String) {
    let dir = tempfile::tempdir().expect("temp dir");
    let download = dir.path().join("dl");
    std::fs::create_dir_all(&download).expect("download dir");

    let hash = HashedPassword::try_from("password".to_string())
        .expect("hash")
        .to_string();

    let db_path = dir.path().join("test.db");
    let local_user_id_line = if configure_local_user_id {
        "local_user_id = \"alice\"\n"
    } else {
        ""
    };
    let config = format!(
        "[database]\nurl = \"{db}\"\n\n\
         [server]\ndownload_folder = \"{dl}\"\n{local_user_id_line}\n\
         [server.authorized_users.alice]\npassword = \"{hash}\"\nroles = [\"owner\"]\n",
        db = db_path.display(),
        dl = download.display(),
    );
    let config_path = dir.path().join("read-flow.toml");
    std::fs::write(&config_path, config).expect("write config");

    let router = server::build_app(config_path.clone())
        .await
        .expect("build router");

    // Seed one file directly (building the router ran the migrations).
    let pool = sqlx::SqlitePool::connect(&format!("sqlite:{}", db_path.display()))
        .await
        .expect("connect");
    let guid = uuid::Uuid::new_v4().to_string();
    let fingerprint = "fp-local-user-id".to_string();
    sqlx::query("INSERT INTO contents (fingerprint) VALUES (?)")
        .bind(&fingerprint)
        .execute(&pool)
        .await
        .expect("insert content");
    sqlx::query(
        r#"INSERT INTO files (guid, path, "type", size, fingerprint, imported_at)
           VALUES (?, ?, 'pdf', 3, ?, '2026-01-01T00:00:00Z')"#,
    )
    .bind(&guid)
    .bind(dir.path().join("book.pdf").display().to_string())
    .bind(&fingerprint)
    .execute(&pool)
    .await
    .expect("insert file");
    pool.close().await;

    (router, dir, config_path, guid, fingerprint)
}

fn basic(user: &str, password: &str) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"));
    format!("Basic {encoded}")
}

async fn send(router: &Router, request: Request<Body>) -> (StatusCode, String) {
    let response = router.clone().oneshot(request).await.expect("response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    (status, String::from_utf8(bytes.to_vec()).expect("utf8"))
}

async fn get_reading_state_as(
    router: &Router,
    user: &str,
    password: &str,
    fingerprint: &str,
) -> (StatusCode, String) {
    let request = Request::builder()
        .uri(format!("/reading-state/{fingerprint}"))
        .header(header::AUTHORIZATION, basic(user, password))
        .body(Body::empty())
        .unwrap();
    send(router, request).await
}

/// Reading progress recorded through the local (unauthenticated) `DbClient`
/// path — the same one COSMIC's GUI uses — becomes visible to `alice`'s
/// authenticated REST session once `server.local_user_id = "alice"` is
/// configured.
#[tokio::test]
async fn local_writes_are_visible_to_the_designated_user_over_rest() {
    let (router, _dir, config_path, _guid, fingerprint) = test_setup(true).await;

    // Simulate COSMIC's local write path: a fresh `ApplicationModule` over
    // the same config/database, writing through `DbClient` (no auth, no
    // `vis`), exactly like the desktop GUI does.
    let module = ApplicationModule::instantiate(config_path)
        .await
        .expect("instantiate module");
    let db_client = module.db_client().await;
    db_client
        .upsert_reading_state(ReadingState {
            fingerprint: fingerprint.clone(),
            status: 0,
            position: "chapter-3".to_string(),
            percentage: 0.42,
            last_updated: "2026-01-02T00:00:00Z".to_string(),
            status_updated_at: "2026-01-02T00:00:00Z".to_string(),
        })
        .await
        .expect("upsert reading state locally");

    // `alice`, authenticated over REST, sees that same progress.
    let (status, body) = get_reading_state_as(&router, "alice", "password", &fingerprint).await;
    Assert::that(status).is(StatusCode::OK);
    let json: serde_json::Value = serde_json::from_str(&body).expect("json");
    Assert::that(json["position"].as_str()).is_some("chapter-3");
    Assert::that(json["percentage"].as_f64()).is_some(0.42);
}

/// Control case: without `server.local_user_id` configured, local writes
/// stay under the reserved `local` user and are invisible to `alice` over
/// REST — today's (unchanged) behaviour.
#[tokio::test]
async fn local_writes_stay_private_without_local_user_id_configured() {
    let (router, _dir, config_path, _guid, fingerprint) = test_setup(false).await;

    let module = ApplicationModule::instantiate(config_path)
        .await
        .expect("instantiate module");
    let db_client = module.db_client().await;
    db_client
        .upsert_reading_state(ReadingState {
            fingerprint: fingerprint.clone(),
            status: 0,
            position: "chapter-3".to_string(),
            percentage: 0.42,
            last_updated: "2026-01-02T00:00:00Z".to_string(),
            status_updated_at: "2026-01-02T00:00:00Z".to_string(),
        })
        .await
        .expect("upsert reading state locally");

    let (status, _) = get_reading_state_as(&router, "alice", "password", &fingerprint).await;
    Assert::that(status).is(StatusCode::NOT_FOUND);
}

/// Configuring `server.local_user_id` on a *running* app (e.g. via COSMIC's
/// Preferences → Server → Local Identity "Save" button, which calls
/// `save_settings`) must take effect for local writes made afterwards,
/// without requiring an app restart. `db_client()` bakes in the resolved
/// user id at cache-build time, so `save_settings`/`update_settings` must
/// invalidate that cache too — not just the settings cache — or local
/// access keeps acting as the stale (previously resolved) user until
/// something calls `reload_settings` (e.g. a restart).
#[tokio::test]
async fn changing_local_user_id_via_save_settings_takes_effect_without_restart() {
    let (router, _dir, config_path, _guid, fingerprint) = test_setup(false).await;

    let module = ApplicationModule::instantiate(config_path)
        .await
        .expect("instantiate module");

    // Before configuring local_user_id: local writes go under the reserved
    // `local` id, invisible to alice (today's default behaviour).
    module
        .db_client()
        .await
        .upsert_reading_state(ReadingState {
            fingerprint: fingerprint.clone(),
            status: 0,
            position: "chapter-1".to_string(),
            percentage: 0.1,
            last_updated: "2026-01-02T00:00:00Z".to_string(),
            status_updated_at: "2026-01-02T00:00:00Z".to_string(),
        })
        .await
        .expect("upsert reading state locally, before configuring local_user_id");

    // Configure local_user_id = "alice" on the *live* module, exactly like
    // the GUI Preferences Save button does — no restart, no fresh
    // `ApplicationModule::instantiate`.
    let mut settings = module.settings().await;
    settings.server.local_user_id = Some("alice".to_string());
    module
        .save_settings(&settings)
        .await
        .expect("save updated settings");

    // A local write made *after* the save should now be recorded under
    // alice, and visible to her over REST — without restarting the app.
    module
        .db_client()
        .await
        .upsert_reading_state(ReadingState {
            fingerprint: fingerprint.clone(),
            status: 0,
            position: "chapter-9".to_string(),
            percentage: 0.9,
            last_updated: "2026-01-03T00:00:00Z".to_string(),
            status_updated_at: "2026-01-03T00:00:00Z".to_string(),
        })
        .await
        .expect("upsert reading state locally, after configuring local_user_id");

    let (status, body) = get_reading_state_as(&router, "alice", "password", &fingerprint).await;
    Assert::that(status).is(StatusCode::OK);
    let json: serde_json::Value = serde_json::from_str(&body).expect("json");
    Assert::that(json["position"].as_str()).is_some("chapter-9");
}

/// A `local_user_id` naming a user that no longer exists in
/// `authorized_users` falls back to the reserved local id rather than
/// erroring, so deleting that user doesn't brick local access.
#[tokio::test]
async fn local_user_id_falls_back_when_configured_user_is_deleted() {
    let (_router, dir, config_path, _guid, fingerprint) = test_setup(true).await;

    // Remove `alice` from authorized_users, leaving `local_user_id = "alice"`
    // dangling, then reload settings.
    let db_path = dir.path().join("test.db");
    let download = dir.path().join("dl");
    let config = format!(
        "[database]\nurl = \"{db}\"\n\n\
         [server]\ndownload_folder = \"{dl}\"\nlocal_user_id = \"alice\"\n\
         authorized_users = {{}}\n",
        db = db_path.display(),
        dl = download.display(),
    );
    std::fs::write(&config_path, config).expect("rewrite config");

    let module = ApplicationModule::instantiate(config_path)
        .await
        .expect("instantiate module");
    let db_client = module.db_client().await;
    // Should not panic/error even though `local_user_id` now dangles.
    db_client
        .upsert_reading_state(ReadingState {
            fingerprint: fingerprint.clone(),
            status: 0,
            position: "chapter-1".to_string(),
            percentage: 0.1,
            last_updated: "2026-01-02T00:00:00Z".to_string(),
            status_updated_at: "2026-01-02T00:00:00Z".to_string(),
        })
        .await
        .expect("upsert reading state locally, falling back to the reserved id");
}
