// SPDX-License-Identifier: AGPL-3.0-or-later

//! Activity-history REST handlers. The wire types live in
//! [`crate::activity`] so both the server response and the remote client
//! deserialize the same representation.

use axum::Json;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use serde::Deserialize;
use serde::Serialize;

use super::AppState;
use super::Error;
use super::Result;
use super::authn::AuthorizedUser;
use crate::activity::ActivityCursor;
use crate::activity::ActivityEvent;
use crate::activity::ActivityOperation;
use crate::db::dao;

#[derive(Debug, Deserialize)]
pub(super) struct ActivityQuery {
    pub limit: Option<usize>,
    pub cursor_started_at: Option<String>,
    pub cursor_operation_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct ActivityPageResponse {
    pub operations: Vec<ActivityOperation>,
    pub next_cursor: Option<ActivityCursor>,
}

#[derive(Debug, Serialize)]
pub(super) struct ActivityDetailResponse {
    #[serde(flatten)]
    pub operation: ActivityOperation,
    pub events: Vec<ActivityEvent>,
}

/// @feature: admin.activity_history
pub(super) async fn list_activity(
    State(state): State<AppState>,
    user: AuthorizedUser,
    Query(query): Query<ActivityQuery>,
) -> Result<Json<ActivityPageResponse>> {
    super::require_owner(&user)?;
    let cursor = match (query.cursor_started_at, query.cursor_operation_id) {
        (Some(started_at), Some(operation_id)) => Some(dao::AuditCursor {
            started_at,
            operation_id,
        }),
        (None, None) => None,
        _ => {
            return Err(Error::BadRequest(
                "activity cursor is incomplete".to_string(),
            ));
        }
    };
    let page = dao::list_audit_operations(
        &state.connection_pool().await,
        query.limit.unwrap_or(50),
        cursor.as_ref(),
    )
    .await?;
    Ok(Json(ActivityPageResponse {
        operations: page
            .operations
            .into_iter()
            .map(ActivityOperation::from)
            .collect(),
        next_cursor: page.next_cursor.map(|cursor| ActivityCursor {
            started_at: cursor.started_at,
            operation_id: cursor.operation_id,
        }),
    }))
}

/// @feature: admin.activity_history
pub(super) async fn get_activity(
    State(state): State<AppState>,
    user: AuthorizedUser,
    Path(operation_id): Path<String>,
) -> Result<Json<ActivityDetailResponse>> {
    super::require_owner(&user)?;
    let pool = state.connection_pool().await;
    let operation = dao::select_audit_operation(&pool, &operation_id)
        .await?
        .ok_or_else(|| Error::FileNotFound(operation_id.clone()))?;
    let events = dao::select_audit_events(&pool, &operation_id).await?;
    Ok(Json(ActivityDetailResponse {
        operation: ActivityOperation::from(operation),
        events: events.into_iter().map(ActivityEvent::from).collect(),
    }))
}

/// @feature: documents.activity_history
pub(super) async fn get_document_activity(
    State(state): State<AppState>,
    user: AuthorizedUser,
    Path(document_id): Path<String>,
) -> Result<Json<Vec<ActivityDetailResponse>>> {
    super::require_owner(&user)?;
    let pool = state.connection_pool().await;
    let operation_ids =
        dao::find_audit_operations_by_target(&pool, "document", &document_id).await?;
    let mut details = Vec::with_capacity(operation_ids.len());
    for operation_id in operation_ids {
        let Some(operation) = dao::select_audit_operation(&pool, &operation_id).await? else {
            continue;
        };
        let events = dao::select_audit_events(&pool, &operation_id).await?;
        details.push(ActivityDetailResponse {
            operation: ActivityOperation::from(operation),
            events: events.into_iter().map(ActivityEvent::from).collect(),
        });
    }
    Ok(Json(details))
}
