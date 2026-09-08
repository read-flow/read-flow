//! Task 6: the audit trail is reachable through the REST API.
//!
//! Mutations issued over HTTP (tag add/remove, reading status, metadata,
//! delete, merge, check-missing) must land in the audit trail with the
//! *authenticated requester* as actor and channel `rest`; the activity
//! endpoints must be owner-only and expose what happened per document — even
//! after that document's row is gone (merge).
//!
//! Drives the real router in-process via `tower::ServiceExt::oneshot`.
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
use serde_json::Value;
use serde_json::json;
use tower::ServiceExt;

/// Build a router with an `owner` (with a `reader`) and an empty database.
/// Returns the router and the temp dir owning the DB file / download folder.
async fn test_router() -> (Router, tempfile::TempDir) {
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
         [server.authorized_users.reader]\npassword = \"{reader}\"\n",
        db = db_path.display(),
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

fn basic(user: &str) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(format!("{user}:password"));
    format!("Basic {encoded}")
}

async fn send(
    router: &Router,
    method: &str,
    uri: &str,
    user: &str,
    body: Option<&Value>,
) -> (StatusCode, String) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, basic(user));
    let body = match body {
        Some(json) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(serde_json::to_vec(json).expect("serialize"))
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

async fn seed_file(
    dir: &tempfile::TempDir,
    db_path: &std::path::Path,
    guid: &str,
    fingerprint: &str,
    name: &str,
    on_disk: bool,
) {
    let pool = sqlx::SqlitePool::connect(&format!("sqlite:{}", db_path.display()))
        .await
        .expect("connect");
    if on_disk {
        std::fs::write(dir.path().join(name), b"file content").expect("write file");
    }
    sqlx::query("INSERT INTO contents (fingerprint) VALUES (?)")
        .bind(fingerprint)
        .execute(&pool)
        .await
        .expect("insert content");
    sqlx::query(
        r#"INSERT INTO files (guid, path, "type", size, fingerprint, imported_at)
           VALUES (?, ?, 'epub', 12, ?, '2026-01-01T00:00:00Z')"#,
    )
    .bind(guid)
    .bind(dir.path().join(name).display().to_string())
    .bind(fingerprint)
    .execute(&pool)
    .await
    .expect("insert file");
    pool.close().await;
}

#[tokio::test]
async fn mutations_via_rest_are_audited_with_the_requester_as_actor() {
    let (router, dir) = test_router().await;
    let guid = "guid-1".to_string();
    seed_file(
        &dir,
        &dir.path().join("test.db"),
        &guid,
        "fp-1",
        "book.epub",
        false,
    )
    .await;

    let (status, body) = send(
        &router,
        "POST",
        &format!("/files/{guid}/document"),
        "owner",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let doc: Value = serde_json::from_str(&body).unwrap();
    let doc_guid = doc["guid"].as_str().unwrap().to_string();
    let (status, _) = send(
        &router,
        "POST",
        &format!("/files/{guid}/tags"),
        "owner",
        Some(&json!(["fiction"])),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = send(
        &router,
        "PUT",
        "/reading-state/fp-1/status",
        "owner",
        Some(&json!({"status": "Reading"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = send(
        &router,
        "PUT",
        &format!("/documents/{doc_guid}/metadata"),
        "owner",
        Some(&json!({"title": "Renamed"})),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Owner sees the full listing. Channel is rest, actor is the requester.
    let (status, body) = send(&router, "GET", "/activity?limit=50", "owner", None).await;
    assert_eq!(status, StatusCode::OK);
    let listing: Value = serde_json::from_str(&body).unwrap();
    let ops = listing["operations"].as_array().unwrap();
    assert!(ops.len() >= 3, "expected ≥3 operations, got {}", ops.len());

    let mut tag_detail: Option<Value> = None;
    let mut status_event: Option<Value> = None;
    let mut metadata_event: Option<Value> = None;
    for op in ops {
        let detail = doc_activity_detail(&router, op["id"].as_str().unwrap()).await;
        for event in detail["events"].as_array().unwrap() {
            match event["event_type"].as_str().unwrap() {
                "document.tags_added" if op["operation_type"] == "tag_edit" => {
                    tag_detail.get_or_insert_with(|| detail.clone());
                }
                "document.status_changed" => {
                    status_event.get_or_insert_with(|| event.clone());
                }
                "document.metadata_changed" => {
                    metadata_event.get_or_insert_with(|| event.clone());
                }
                _ => {}
            }
        }
    }
    let tag_detail = tag_detail.expect("tag_edit operation with tags_added event");
    // The actor is the authenticated requester, and the channel is `rest`.
    assert_eq!(tag_detail["actor"]["kind"], "user");
    assert_eq!(tag_detail["actor"]["id"], "owner");
    assert_eq!(tag_detail["channel"], "rest");
    let tag_event = tag_detail["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["event_type"] == "document.tags_added")
        .unwrap();
    assert_eq!(tag_event["outcome"], "success");
    assert_eq!(tag_event["parameters"]["tags"][0], "fiction");

    let status_event = status_event.expect("status operation");
    assert_eq!(status_event["parameters"]["status"], "reading");

    let metadata_event = metadata_event.expect("metadata operation");
    assert_eq!(metadata_event["outcome"], "success");
    // Metadata is captured as a target snapshot, not a parameter.
    assert_eq!(metadata_event["targets"][0]["title_snapshot"], "Renamed");

    // Per-document activity: metadata/status events target the document.
    let (status, body) = send(
        &router,
        "GET",
        &format!("/documents/{doc_guid}/activity"),
        "owner",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let details: Value = serde_json::from_str(&body).unwrap();
    let metadata_changed = details
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|d| d["events"].as_array().unwrap())
        .any(|e| e["event_type"] == "document.metadata_changed");
    assert!(
        metadata_changed,
        "document activity must include metadata change"
    );

    // A non-owner cannot read the audit trail.
    let (status, _) = send(&router, "GET", "/activity", "reader", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn reading_status_via_rest_is_scoped_to_the_requesting_user() {
    // Regression: the audited status update must write the reading_state row
    // for the authenticated requester, not the embedded app's local user —
    // otherwise the owner's follow-up reads keep seeing `Unread`.
    let (router, dir) = test_router().await;
    let guid = "guid-status".to_string();
    seed_file(
        &dir,
        &dir.path().join("test.db"),
        &guid,
        "fp-status",
        "book.epub",
        true,
    )
    .await;

    let (status, _) = send(
        &router,
        "PUT",
        "/reading-state/fp-status/status",
        "owner",
        Some(&json!({ "status": "Reading" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = send(&router, "GET", &format!("/files/{guid}"), "owner", None).await;
    assert_eq!(status, StatusCode::OK);
    let file: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        file["status"], "Reading",
        "owner must read back its own status"
    );

    // A second user has no reading state of its own for this file.
    let (status, _) = send(&router, "GET", "/reading-state/fp-status", "reader", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_file_via_rest_records_file_deleted_with_target() {
    let (router, dir) = test_router().await;
    let guid = "guid-del".to_string();
    seed_file(
        &dir,
        &dir.path().join("test.db"),
        &guid,
        "fp-del",
        "book.epub",
        true,
    )
    .await;

    let (status, _) = send(&router, "DELETE", &format!("/files/{guid}"), "owner", None).await;
    assert_eq!(status, StatusCode::OK);

    let (_, body) = send(&router, "GET", "/activity?limit=10", "owner", None).await;
    let listing: Value = serde_json::from_str(&body).unwrap();
    let delete_op = listing["operations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|op| op["operation_type"] == "file_delete")
        .expect("file_delete operation");
    let detail = doc_activity_detail(&router, delete_op["id"].as_str().unwrap()).await;
    let event = detail["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["event_type"] == "file.deleted")
        .expect("file.deleted event");
    assert_eq!(event["outcome"], "success");
    let target = event["targets"][0].as_object().unwrap();
    assert_eq!(target["target_kind"], "file");
    assert_eq!(target["target_id"], guid);
    assert_eq!(
        target["path_snapshot"],
        dir.path().join("book.epub").display().to_string()
    );
}

#[tokio::test]
async fn merge_via_rest_keeps_loser_activity_findable() {
    let (router, dir) = test_router().await;
    let winner_guid = "guid-winner".to_string();
    let loser_guid = "guid-loser".to_string();
    seed_file(
        &dir,
        &dir.path().join("test.db"),
        &winner_guid,
        "fp-winner",
        "winner.epub",
        false,
    )
    .await;
    seed_file(
        &dir,
        &dir.path().join("test.db"),
        &loser_guid,
        "fp-loser",
        "loser.epub",
        false,
    )
    .await;
    let mut winner_doc = String::new();
    let mut loser_doc = String::new();
    for (guid, target) in [
        (&winner_guid, &mut winner_doc),
        (&loser_guid, &mut loser_doc),
    ] {
        let (status, body) = send(
            &router,
            "POST",
            &format!("/files/{guid}/document"),
            "owner",
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let doc: Value = serde_json::from_str(&body).unwrap();
        *target = doc["guid"].as_str().unwrap().to_string();
    }

    let (status, _) = send(
        &router,
        "POST",
        "/documents/merge",
        "owner",
        Some(&json!({
            "winner_guid": winner_doc,
            "loser_guids": [loser_doc],
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // The loser row is gone (merged), but its activity remains findable.
    let (status, body) = send(
        &router,
        "GET",
        &format!("/documents/{loser_doc}/activity"),
        "owner",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let details: Value = serde_json::from_str(&body).unwrap();
    let merge_ops = details
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["operation_type"] == "document_merge");
    let events: Vec<&Value> = merge_ops
        .flat_map(|d| d["events"].as_array().unwrap())
        .filter(|e| e["event_type"] == "document.merged")
        .collect();
    assert!(!events.is_empty(), "loser activity must contain the merge");
    let event = events[0];
    assert_eq!(event["outcome"], "success");
    assert_eq!(event["parameters"]["winner_guid"], winner_doc);
    assert_eq!(event["parameters"]["loser_count"], 1);
    // The event is indexed under the loser's document target.
    assert!(
        event["targets"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["target_kind"] == "document" && t["target_id"] == loser_doc)
    );
}

#[tokio::test]
async fn check_missing_purge_via_rest_records_maintenance() {
    let (router, dir) = test_router().await;
    // The seeded row points at a file that no longer exists.
    let guid = "guid-gone".to_string();
    seed_file(
        &dir,
        &dir.path().join("test.db"),
        &guid,
        "fp-gone",
        "gone.epub",
        false,
    )
    .await;

    let (status, body) = send(
        &router,
        "POST",
        "/maintenance/check-missing?purge=true",
        "owner",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let response: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(response["missing"].as_array().unwrap().len(), 1);

    let (_, body) = send(&router, "GET", "/activity?limit=10", "owner", None).await;
    let listing: Value = serde_json::from_str(&body).unwrap();
    let mut maintenance_events: Vec<Value> = Vec::new();
    for op in listing["operations"].as_array().unwrap() {
        if op["operation_type"] != "missing_file_maintenance" {
            continue;
        }
        let (detail_status, detail_body) = send(
            &router,
            "GET",
            &format!("/activity/{}", op["id"].as_str().unwrap()),
            "owner",
            None,
        )
        .await;
        assert_eq!(detail_status, StatusCode::OK);
        let detail: Value = serde_json::from_str(&detail_body).unwrap();
        maintenance_events.extend(
            detail["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| e["event_type"] == "maintenance.missing_files_purged")
                .cloned(),
        );
    }
    assert_eq!(maintenance_events.len(), 1);
    let event = &maintenance_events[0];
    assert_eq!(event["parameters"]["purge"], true);
    assert_eq!(event["parameters"]["missing_at_recorded_path"], 1);
}

/// Fetch and parse `/activity/{id}` (assumed fine for the owner).
async fn doc_activity_detail(router: &Router, operation_id: &str) -> Value {
    let (status, body) = send(
        router,
        "GET",
        &format!("/activity/{operation_id}"),
        "owner",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    serde_json::from_str(&body).unwrap()
}
