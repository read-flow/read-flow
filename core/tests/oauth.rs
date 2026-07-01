//! End-to-end auth flow against the real router: exchange Basic credentials for
//! a Bearer JWT at `/oauth/token`, then use the token on protected endpoints.
//! Runs in-process (no subprocess) by driving the `axum::Router` with
//! `tower::ServiceExt::oneshot`.
#![cfg(feature = "server")]

use std::path::PathBuf;

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

/// Build a router backed by a fresh temp SQLite db and two users:
/// `owner` (role `owner`) and `reader` (no roles). Password is `password`.
async fn test_router() -> (Router, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let download = dir.path().join("dl");
    std::fs::create_dir_all(&download).expect("download dir");

    // Hashing once here is fine (the whole point is to avoid it *per request*).
    let hash = |p: &str| {
        HashedPassword::try_from(p.to_string())
            .expect("hash")
            .to_string()
    };

    let config = format!(
        "[database]\nurl = \"{db}\"\n\n\
         [server]\ndownload_folder = \"{dl}\"\n\n\
         [server.authorized_users.owner]\npassword = \"{owner}\"\nroles = [\"owner\"]\n\n\
         [server.authorized_users.reader]\npassword = \"{reader}\"\n",
        db = dir.path().join("test.db").display(),
        dl = download.display(),
        owner = hash("password"),
        reader = hash("password"),
    );
    let config_path = dir.path().join("read-flow.toml");
    std::fs::write(&config_path, config).expect("write config");

    let router = server::build_app(PathBuf::from(&config_path))
        .await
        .expect("build router");
    (router, dir)
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

/// Exchange credentials at `/oauth/token` and return the access token.
async fn obtain_token(router: &Router, user: &str, password: &str) -> String {
    let request = Request::builder()
        .method("POST")
        .uri("/oauth/token")
        .header(header::AUTHORIZATION, basic(user, password))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("grant_type=password"))
        .unwrap();
    let (status, body) = send(router, request).await;
    assert_eq!(status, StatusCode::OK, "token exchange body: {body}");
    let json: serde_json::Value = serde_json::from_str(&body).expect("json");
    assert_eq!(json["token_type"], "Bearer");
    json["access_token"]
        .as_str()
        .expect("access_token")
        .to_string()
}

fn get_status(bearer: Option<&str>, basic_header: Option<String>) -> Request<Body> {
    let mut builder = Request::builder().uri("/status");
    if let Some(bearer) = bearer {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {bearer}"));
    } else if let Some(basic) = basic_header {
        builder = builder.header(header::AUTHORIZATION, basic);
    }
    builder.body(Body::empty()).unwrap()
}

#[tokio::test]
async fn token_grants_access_and_encodes_roles() {
    let (router, _dir) = test_router().await;

    // Exchange → token; the scope claim mirrors the owner role.
    let token = obtain_token(&router, "owner", "password").await;

    // Bearer token is accepted on a protected endpoint.
    let (status, _) = send(&router, get_status(Some(&token), None)).await;
    assert_eq!(status, StatusCode::OK);

    // Owner-only endpoint works with the token — roles came from the JWT.
    let users = Request::builder()
        .uri("/users")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&router, users).await.0, StatusCode::OK);
}

#[tokio::test]
async fn non_owner_token_is_forbidden_on_admin_endpoint() {
    let (router, _dir) = test_router().await;
    let token = obtain_token(&router, "reader", "password").await;

    // Token is valid (authenticated) but lacks the owner role → 403, not 401.
    let users = Request::builder()
        .uri("/users")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&router, users).await.0, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn basic_still_works_and_bad_token_is_rejected() {
    let (router, _dir) = test_router().await;

    // Basic auth remains accepted (additive change).
    let (status, _) = send(&router, get_status(None, Some(basic("owner", "password")))).await;
    assert_eq!(status, StatusCode::OK);

    // A garbage bearer token → 401 with a Bearer challenge.
    let response = router
        .clone()
        .oneshot(get_status(Some("not-a-jwt"), None))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .and_then(|v| v.to_str().ok()),
        Some("Bearer error=\"invalid_token\""),
    );

    // No credentials at all → 401.
    let (status, _) = send(&router, get_status(None, None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn oversized_upload_is_rejected() {
    // Build a router with a tiny upload cap.
    let dir = tempfile::tempdir().expect("temp dir");
    let download = dir.path().join("dl");
    std::fs::create_dir_all(&download).expect("download dir");
    let hash = HashedPassword::try_from("password".to_string())
        .expect("hash")
        .to_string();
    let config = format!(
        "[database]\nurl = \"{db}\"\n\n\
         [server]\ndownload_folder = \"{dl}\"\nmax_upload_bytes = 16\n\n\
         [server.authorized_users.owner]\npassword = \"{hash}\"\nroles = [\"owner\"]\n",
        db = dir.path().join("test.db").display(),
        dl = download.display(),
    );
    let config_path = dir.path().join("read-flow.toml");
    std::fs::write(&config_path, config).expect("write config");
    let router = server::build_app(PathBuf::from(&config_path))
        .await
        .expect("router");

    // A body well over the 16-byte cap → 413 before the handler runs.
    let request = Request::builder()
        .method("POST")
        .uri("/files")
        .header(header::AUTHORIZATION, basic("owner", "password"))
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_LENGTH, "1024")
        .body(Body::from(vec![0u8; 1024]))
        .unwrap();
    let (status, _) = send(&router, request).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn bad_credentials_yield_invalid_grant() {
    let (router, _dir) = test_router().await;
    let request = Request::builder()
        .method("POST")
        .uri("/oauth/token")
        .header(header::AUTHORIZATION, basic("owner", "wrong"))
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("grant_type=password"))
        .unwrap();
    let (status, body) = send(&router, request).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let json: serde_json::Value = serde_json::from_str(&body).expect("json");
    assert_eq!(json["error"], "invalid_grant");
}
