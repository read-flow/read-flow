//! Regression tests for the centralized content-visibility policy
//! (`server::access::Visibility`).
//!
//! Private (hidden-tag) content must be invisible — and immutable — through
//! every content endpoint unless the request carries `x-private-mode: true`
//! *and* the user has the `owner` role. These tests drive the real router
//! in-process via `tower::ServiceExt::oneshot`.
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
use read_flow_core::server;
use read_flow_core::settings::HashedPassword;
use tower::ServiceExt;

const SECRET_TAG: &str = "secret";

/// Build a router with a private tag configured, plus one visible and one
/// hidden file seeded directly into the SQLite database.
///
/// Returns the router and the guids/fingerprints of the seeded files:
/// `(router, dir, visible_guid, hidden_guid, visible_fp, hidden_fp)`.
async fn test_router() -> (Router, tempfile::TempDir, String, String, String, String) {
    let dir = tempfile::tempdir().expect("temp dir");
    let download = dir.path().join("dl");
    std::fs::create_dir_all(&download).expect("download dir");

    let hash = |p: &str| {
        HashedPassword::try_from(p.to_string())
            .expect("hash")
            .to_string()
    };
    let db_path = dir.path().join("test.db");
    let config = format!(
        "[database]\nurl = \"{db}\"\n\n\
         [server]\ndownload_folder = \"{dl}\"\n\n\
         [server.authorized_users.owner]\npassword = \"{owner}\"\nroles = [\"owner\"]\n\n\
         [server.authorized_users.reader]\npassword = \"{reader}\"\n\n\
         [ui]\nprivate_tags = [\"{tag}\"]\n",
        db = db_path.display(),
        dl = download.display(),
        owner = hash("password"),
        reader = hash("password"),
        tag = SECRET_TAG,
    );
    let config_path = dir.path().join("read-flow.toml");
    std::fs::write(&config_path, config).expect("write config");

    let router = server::build_app(PathBuf::from(&config_path))
        .await
        .expect("build router");

    // Seed two files directly (building the router ran the migrations).
    let pool = sqlx::SqlitePool::connect(&format!("sqlite:{}", db_path.display()))
        .await
        .expect("connect");
    let visible_guid = uuid::Uuid::new_v4().to_string();
    let hidden_guid = uuid::Uuid::new_v4().to_string();
    let visible_fp = "fp-visible".to_string();
    let hidden_fp = "fp-hidden".to_string();
    for (guid, fp, name) in [
        (&visible_guid, &visible_fp, "visible.pdf"),
        (&hidden_guid, &hidden_fp, "hidden.pdf"),
    ] {
        sqlx::query("INSERT INTO contents (fingerprint) VALUES (?)")
            .bind(fp)
            .execute(&pool)
            .await
            .expect("insert content");
        sqlx::query(
            r#"INSERT INTO files (guid, path, "type", size, fingerprint, imported_at)
               VALUES (?, ?, 'pdf', 3, ?, '2026-01-01T00:00:00Z')"#,
        )
        .bind(guid)
        .bind(dir.path().join(name).display().to_string())
        .bind(fp)
        .execute(&pool)
        .await
        .expect("insert file");
    }
    sqlx::query("INSERT INTO content_tags (fingerprint, tag) VALUES (?, ?)")
        .bind(&hidden_fp)
        .bind(SECRET_TAG)
        .execute(&pool)
        .await
        .expect("tag hidden file");
    pool.close().await;

    (
        router,
        dir,
        visible_guid,
        hidden_guid,
        visible_fp,
        hidden_fp,
    )
}

fn basic(user: &str) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(format!("{user}:password"));
    format!("Basic {encoded}")
}

struct Req<'a> {
    method: &'a str,
    uri: &'a str,
    user: &'a str,
    private_mode: bool,
    body: Option<serde_json::Value>,
}

async fn send(router: &Router, req: Req<'_>) -> (StatusCode, String) {
    let mut builder = Request::builder()
        .method(req.method)
        .uri(req.uri)
        .header(header::AUTHORIZATION, basic(req.user));
    if req.private_mode {
        builder = builder.header("x-private-mode", "true");
    }
    let body = match req.body {
        Some(json) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(serde_json::to_vec(&json).expect("serialize"))
        }
        None => Body::empty(),
    };
    let response = router
        .clone()
        .oneshot(builder.body(body).unwrap())
        .await
        .expect("response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    (status, String::from_utf8_lossy(&bytes).to_string())
}

#[tokio::test]
async fn private_mode_header_requires_owner_on_every_content_endpoint() {
    let (router, _dir, visible_guid, _, visible_fp, _) = test_router().await;

    let endpoints: Vec<(&str, String, Option<serde_json::Value>)> = vec![
        ("GET", "/files".into(), None),
        ("GET", "/files/tags".into(), None),
        ("GET", format!("/files/{visible_guid}"), None),
        ("GET", format!("/files/{visible_guid}/tags"), None),
        (
            "POST",
            format!("/files/{visible_guid}/tags"),
            Some(serde_json::json!(["a"])),
        ),
        ("GET", format!("/files/{visible_guid}/cover"), None),
        ("GET", "/documents".into(), None),
        ("GET", format!("/reading-state/{visible_fp}"), None),
    ];
    for (method, uri, body) in endpoints {
        let (status, _) = send(
            &router,
            Req {
                method,
                uri: &uri,
                user: "reader",
                private_mode: true,
                body,
            },
        )
        .await;
        Assert::that(status).is(StatusCode::FORBIDDEN);
    }
}

#[tokio::test]
async fn hidden_files_are_invisible_to_normal_requests() {
    let (router, _dir, visible_guid, hidden_guid, _, _) = test_router().await;

    // List: only the visible file shows up (for both users, without the header).
    for user in ["reader", "owner"] {
        let (status, body) = send(
            &router,
            Req {
                method: "GET",
                uri: "/files",
                user,
                private_mode: false,
                body: None,
            },
        )
        .await;
        Assert::that(status).is(StatusCode::OK);
        Assert::that(body.contains(&visible_guid)).is(true);
        Assert::that(body.contains(&hidden_guid)).is(false);
    }

    // Direct fetch of the hidden file 404s like a missing one.
    let (status, _) = send(
        &router,
        Req {
            method: "GET",
            uri: &format!("/files/{hidden_guid}"),
            user: "reader",
            private_mode: false,
            body: None,
        },
    )
    .await;
    Assert::that(status).is(StatusCode::NOT_FOUND);

    // The private tag itself is not listed.
    let (status, body) = send(
        &router,
        Req {
            method: "GET",
            uri: "/files/tags",
            user: "reader",
            private_mode: false,
            body: None,
        },
    )
    .await;
    Assert::that(status).is(StatusCode::OK);
    Assert::that(body.contains(SECRET_TAG)).is(false);
}

#[tokio::test]
async fn owner_with_private_mode_header_sees_hidden_files() {
    let (router, _dir, visible_guid, hidden_guid, _, _) = test_router().await;

    let (status, body) = send(
        &router,
        Req {
            method: "GET",
            uri: "/files",
            user: "owner",
            private_mode: true,
            body: None,
        },
    )
    .await;
    Assert::that(status).is(StatusCode::OK);
    Assert::that(body.contains(&visible_guid)).is(true);
    Assert::that(body.contains(&hidden_guid)).is(true);
}

#[tokio::test]
async fn update_file_cannot_touch_hidden_files() {
    let (router, _dir, _, hidden_guid, _, hidden_fp) = test_router().await;

    // Previously fail-open: any authenticated user could PUT /files for a
    // hidden file. Now it must 404.
    let file = serde_json::json!({
        "guid": hidden_guid,
        "path": "/tmp/replaced.pdf",
        "type_": "pdf",
        "size": 3,
        "fingerprint": hidden_fp,
        "tags": [],
        "status": "Unread",
    });
    let (status, _) = send(
        &router,
        Req {
            method: "PUT",
            uri: "/files",
            user: "reader",
            private_mode: false,
            body: Some(file),
        },
    )
    .await;
    Assert::that(status).is(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn reading_state_of_hidden_content_is_unreachable() {
    let (router, _dir, _, _, _, hidden_fp) = test_router().await;

    // Previously fail-open: reading state ignored private tags entirely.
    let (status, _) = send(
        &router,
        Req {
            method: "GET",
            uri: &format!("/reading-state/{hidden_fp}"),
            user: "reader",
            private_mode: false,
            body: None,
        },
    )
    .await;
    Assert::that(status).is(StatusCode::NOT_FOUND);

    let state = serde_json::json!({
        "fingerprint": hidden_fp,
        "status": 1,
        "position": "p1",
        "percentage": 0.5,
        "last_updated": "2026-01-01T00:00:00Z",
        "status_updated_at": "2026-01-01T00:00:00Z",
    });
    let (status, _) = send(
        &router,
        Req {
            method: "PUT",
            uri: "/reading-state",
            user: "reader",
            private_mode: false,
            body: Some(state),
        },
    )
    .await;
    Assert::that(status).is(StatusCode::NOT_FOUND);

    let (status, _) = send(
        &router,
        Req {
            method: "PUT",
            uri: &format!("/reading-state/{hidden_fp}/status"),
            user: "reader",
            private_mode: false,
            body: Some(serde_json::json!({"status": "Read"})),
        },
    )
    .await;
    Assert::that(status).is(StatusCode::NOT_FOUND);

    // Owner in private mode can still reach it (no stored state yet → 404 is
    // fine, but not Forbidden).
    let (status, _) = send(
        &router,
        Req {
            method: "PUT",
            uri: &format!("/reading-state/{hidden_fp}/status"),
            user: "owner",
            private_mode: true,
            body: Some(serde_json::json!({"status": "Read"})),
        },
    )
    .await;
    Assert::that(status).is(StatusCode::OK);
}

#[tokio::test]
async fn documents_of_hidden_files_are_invisible() {
    let (router, _dir, _, hidden_guid, _, _) = test_router().await;

    // Create a document for the hidden file as the owner in private mode.
    let (status, body) = send(
        &router,
        Req {
            method: "POST",
            uri: &format!("/files/{hidden_guid}/document"),
            user: "owner",
            private_mode: true,
            body: None,
        },
    )
    .await;
    Assert::that(status).is(StatusCode::OK);
    let doc: serde_json::Value = serde_json::from_str(&body).expect("document json");
    let doc_guid = doc["guid"].as_str().expect("guid");

    // A normal request cannot see that document...
    let (status, _) = send(
        &router,
        Req {
            method: "GET",
            uri: &format!("/documents/{doc_guid}"),
            user: "reader",
            private_mode: false,
            body: None,
        },
    )
    .await;
    Assert::that(status).is(StatusCode::NOT_FOUND);

    // ...nor find it in the list...
    let (status, body) = send(
        &router,
        Req {
            method: "GET",
            uri: "/documents",
            user: "reader",
            private_mode: false,
            body: None,
        },
    )
    .await;
    Assert::that(status).is(StatusCode::OK);
    Assert::that(body.contains(doc_guid)).is(false);

    // ...nor edit its metadata (previously fail-open).
    let (status, _) = send(
        &router,
        Req {
            method: "PUT",
            uri: &format!("/documents/{doc_guid}/metadata"),
            user: "reader",
            private_mode: false,
            body: Some(serde_json::json!({"title": "defaced"})),
        },
    )
    .await;
    Assert::that(status).is(StatusCode::NOT_FOUND);
}
