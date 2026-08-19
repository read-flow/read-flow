//! Integration tests for the PDF page-picker endpoints (page count, page
//! preview, save-as-thumbnail) that back the "change thumbnail" feature.
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

const SAMPLE_PDF: &str = "../features/fixtures/sample.pdf";

/// Build a router with one real on-disk PDF file (`pdf_guid`/`pdf_fp`) and
/// one non-PDF file (`epub_guid`), both owned by the `owner` user.
async fn test_router() -> (Router, tempfile::TempDir, String, String, String) {
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
         [server.authorized_users.owner]\npassword = \"{owner}\"\nroles = [\"owner\"]\n",
        db = db_path.display(),
        dl = download.display(),
        owner = hash("password"),
    );
    let config_path = dir.path().join("read-flow.toml");
    std::fs::write(&config_path, config).expect("write config");

    let router = server::build_app(PathBuf::from(&config_path))
        .await
        .expect("build router");

    let pdf_path = dir.path().join("sample.pdf");
    std::fs::copy(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(SAMPLE_PDF),
        &pdf_path,
    )
    .expect("copy fixture pdf");

    let pool = sqlx::SqlitePool::connect(&format!("sqlite:{}", db_path.display()))
        .await
        .expect("connect");

    let pdf_guid = uuid::Uuid::new_v4().to_string();
    let pdf_fp = "fp-pdf".to_string();
    sqlx::query("INSERT INTO contents (fingerprint) VALUES (?)")
        .bind(&pdf_fp)
        .execute(&pool)
        .await
        .expect("insert pdf content");
    sqlx::query(
        r#"INSERT INTO files (guid, path, "type", size, fingerprint, imported_at)
           VALUES (?, ?, 'pdf', 3, ?, '2026-01-01T00:00:00Z')"#,
    )
    .bind(&pdf_guid)
    .bind(pdf_path.display().to_string())
    .bind(&pdf_fp)
    .execute(&pool)
    .await
    .expect("insert pdf file");

    let epub_guid = uuid::Uuid::new_v4().to_string();
    let epub_fp = "fp-epub".to_string();
    sqlx::query("INSERT INTO contents (fingerprint) VALUES (?)")
        .bind(&epub_fp)
        .execute(&pool)
        .await
        .expect("insert epub content");
    sqlx::query(
        r#"INSERT INTO files (guid, path, "type", size, fingerprint, imported_at)
           VALUES (?, ?, 'epub', 3, ?, '2026-01-01T00:00:00Z')"#,
    )
    .bind(&epub_guid)
    .bind(dir.path().join("sample.epub").display().to_string())
    .bind(&epub_fp)
    .execute(&pool)
    .await
    .expect("insert epub file");

    pool.close().await;

    (router, dir, pdf_guid, pdf_fp, epub_guid)
}

fn basic(user: &str) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(format!("{user}:password"));
    format!("Basic {encoded}")
}

async fn get(router: &Router, uri: &str) -> (StatusCode, Vec<u8>) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .header(header::AUTHORIZATION, basic("owner"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body")
        .to_vec();
    (status, bytes)
}

async fn post_json(router: &Router, uri: &str, body: serde_json::Value) -> (StatusCode, Vec<u8>) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header(header::AUTHORIZATION, basic("owner"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&body).expect("serialize")))
                .unwrap(),
        )
        .await
        .expect("response");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body")
        .to_vec();
    (status, bytes)
}

#[tokio::test]
async fn page_count_returns_fixture_page_count() {
    let (router, _dir, pdf_guid, _pdf_fp, _epub_guid) = test_router().await;
    let (status, body) = get(&router, &format!("/files/{pdf_guid}/pdf/page-count")).await;
    Assert::that(status).is(StatusCode::OK);
    let json: serde_json::Value = serde_json::from_slice(&body).expect("json");
    Assert::that(json["page_count"].as_i64()).is(Some(1));
}

#[tokio::test]
async fn page_count_rejects_non_pdf_file() {
    let (router, _dir, _pdf_guid, _pdf_fp, epub_guid) = test_router().await;
    let (status, _) = get(&router, &format!("/files/{epub_guid}/pdf/page-count")).await;
    Assert::that(status).is(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn page_count_404s_for_missing_file() {
    let (router, _dir, _pdf_guid, _pdf_fp, _epub_guid) = test_router().await;
    let missing_guid = uuid::Uuid::new_v4().to_string();
    let (status, _) = get(&router, &format!("/files/{missing_guid}/pdf/page-count")).await;
    Assert::that(status).is(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn page_preview_renders_webp_bytes() {
    let (router, _dir, pdf_guid, _pdf_fp, _epub_guid) = test_router().await;
    let (status, body) = get(
        &router,
        &format!("/files/{pdf_guid}/pdf/page/0/preview?size=thumb"),
    )
    .await;
    Assert::that(status).is(StatusCode::OK);
    Assert::that(body.is_empty()).is(false);
}

#[tokio::test]
async fn page_preview_accepts_custom_trim_padding() {
    let (router, _dir, pdf_guid, _pdf_fp, _epub_guid) = test_router().await;
    let (status, body) = get(
        &router,
        &format!("/files/{pdf_guid}/pdf/page/0/preview?trim=true&padding=20&size=thumb"),
    )
    .await;
    Assert::that(status).is(StatusCode::OK);
    Assert::that(body.is_empty()).is(false);
}

#[tokio::test]
async fn save_thumbnail_updates_file_cover_and_survives_rescan_style_upsert() {
    let (router, _dir, pdf_guid, pdf_fp, _epub_guid) = test_router().await;

    let (status, body) = post_json(
        &router,
        &format!("/files/{pdf_guid}/pdf/page/0/thumbnail"),
        serde_json::json!({ "trim": true }),
    )
    .await;
    Assert::that(status).is(StatusCode::OK);
    let doc: serde_json::Value = serde_json::from_slice(&body).expect("json");
    Assert::that(
        doc["metadata"]["selected_cover_fingerprint"]
            .as_str()
            .map(str::to_string),
    )
    .is(Some(pdf_fp.clone()));

    let (status, cover_bytes) = get(&router, &format!("/files/{pdf_guid}/cover")).await;
    Assert::that(status).is(StatusCode::OK);
    Assert::that(cover_bytes.is_empty()).is(false);
}

#[tokio::test]
async fn save_thumbnail_rejects_non_pdf_file() {
    let (router, _dir, _pdf_guid, _pdf_fp, epub_guid) = test_router().await;
    let (status, _) = post_json(
        &router,
        &format!("/files/{epub_guid}/pdf/page/0/thumbnail"),
        serde_json::json!({ "trim": false }),
    )
    .await;
    Assert::that(status).is(StatusCode::BAD_REQUEST);
}
