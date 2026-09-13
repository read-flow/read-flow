// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use sqlx::Row;
use sqlx::SqliteConnection;

use super::ConnectionPool;
use super::dao;
use super::dao::Error;
use crate::api::ApiDocument;
use crate::api::DocumentMeta;
use crate::api::File;
use crate::api::FileDataSource;
use crate::api::ReadingState;
use crate::api::ReadingStatus;
use crate::api::Status;
use crate::audit::AuditActor;
use crate::audit::AuditChannel;
use crate::audit::AuditContext;
use crate::audit::AuditEventType;
use crate::audit::AuditOutcome;
use crate::audit::OperationStatus;
use crate::audit::OperationType;
use crate::db::models::ContentTag;
use crate::db::models::NewFile;
use crate::scan::metadata::ExtractedMetadata;

/// Extract an archive member to a stable temp location (keyed by file guid)
/// so it can be opened with the system default application. Repeat opens
/// reuse the previously extracted copy.
async fn extract_member_to_cache(
    archive_path: &str,
    inner: &str,
    guid: &str,
    extension: &str,
) -> Result<std::path::PathBuf, Error> {
    let archive_path = archive_path.to_owned();
    let inner = inner.to_owned();
    let guid = guid.to_owned();
    let extension = extension.to_owned();
    tokio::task::spawn_blocking(move || {
        crate::scan::archive::extract_member_to_cache(
            Path::new(&archive_path),
            &inner,
            &guid,
            &extension,
        )
    })
    .await
    .map_err(|e| Error::IO(Arc::new(std::io::Error::other(e))))?
    .map_err(|e| Error::IO(Arc::new(e)))
}

#[derive(Clone)]
pub struct DbClient {
    connection_pool: ConnectionPool,
    /// The user id local (COSMIC GUI/CLI) access is recorded under. Resolved
    /// from [`crate::settings::ServerSettings::resolve_local_user_id`] —
    /// [`crate::db::LOCAL_USER_ID`] by default, or a designated authorized
    /// user when `server.local_user_id` is configured, so local reading
    /// state/tags are shared with that user's REST/PWA sessions.
    user_id: String,
}

impl DbClient {
    pub fn new(connection_pool: ConnectionPool, user_id: String) -> Self {
        Self {
            connection_pool,
            user_id,
        }
    }
}

#[async_trait::async_trait]
impl FileDataSource for DbClient {
    type Error = Error;

    fn display_name(&self) -> String {
        "Local Files".to_string()
    }

    async fn status(&self) -> Result<Status, Self::Error> {
        let mut conn = self.connection_pool.acquire().await?;
        sqlx::query("SELECT 1").execute(&mut *conn).await?;
        Ok(Status {
            identifier: "database".to_string(),
            ..Default::default()
        })
    }

    async fn get_files(&self) -> Result<Vec<File>, Self::Error> {
        let mut conn = self.connection_pool.acquire().await?;
        let files = dao::select_all_files(&mut conn, &self.user_id).await?;
        let all_tags = dao::select_all_content_tags(&mut conn).await?;
        let cover_fps = dao::select_fingerprints_with_covers(&mut conn).await?;

        let mut tags_by_fp: HashMap<String, Vec<ContentTag>> = HashMap::new();
        for tag in all_tags {
            tags_by_fp
                .entry(tag.fingerprint.clone())
                .or_default()
                .push(tag);
        }

        Ok(files
            .into_iter()
            .map(|file| {
                let tags = tags_by_fp.remove(&file.fingerprint).unwrap_or_default();
                let has_cover = cover_fps.contains(&file.fingerprint);
                let mut api_file: File = (file, tags).into();
                api_file.has_cover = has_cover;
                api_file
            })
            .collect())
    }

    async fn get_files_tags(&self) -> Result<Vec<String>, Self::Error> {
        let mut conn = self.connection_pool.acquire().await?;
        dao::select_all_distinct_tags(&mut conn).await
    }

    async fn get_file(&self, guid: &str) -> Result<Option<File>, Self::Error> {
        let mut conn = self.connection_pool.acquire().await?;
        let Some(file) = dao::select_file_by_guid(&mut conn, &self.user_id, guid).await? else {
            return Ok(None);
        };
        let tags = dao::select_content_tags_by_fingerprint(&mut conn, &file.fingerprint).await?;
        let has_cover = dao::cover_exists(&mut conn, &file.fingerprint).await?;
        let mut api_file: File = (file, tags).into();
        api_file.has_cover = has_cover;
        Ok(Some(api_file))
    }

    async fn update_file(&self, file: File) -> Result<(), Self::Error> {
        let mut tx = self.connection_pool.begin().await?;

        let Some(existing) = dao::select_file_by_guid(&mut tx, &self.user_id, &file.guid).await?
        else {
            return Ok(());
        };

        // If the fingerprint changed, ensure the new content row exists first.
        if existing.fingerprint != file.fingerprint {
            dao::upsert_content(&mut tx, &file.fingerprint).await?;
        }

        // Update file-level fields (path, type, size, fingerprint).
        let updated = crate::db::models::File {
            id: existing.id,
            guid: existing.guid.clone(),
            path: file.path.clone(),
            type_: file.type_.clone(),
            size: file.size,
            fingerprint: file.fingerprint.clone(),
            archive_path: existing.archive_path.clone(),
            archive_inner_path: existing.archive_inner_path.clone(),
            imported_at: existing.imported_at.clone(),
            status: existing.status,
            document_guid: existing.document_guid.clone(),
        };
        dao::update_file(&mut tx, &updated).await?;

        // Sync content tags: delete removed, upsert added.
        let existing_tags =
            dao::select_content_tags_by_fingerprint(&mut tx, &file.fingerprint).await?;
        let to_delete: Vec<String> = existing_tags
            .iter()
            .filter(|t| !file.tags.contains(&t.tag))
            .map(|t| t.tag.clone())
            .collect();
        dao::delete_content_tags(&mut tx, &file.fingerprint, to_delete).await?;
        let to_add: Vec<ContentTag> = file
            .tags
            .iter()
            .filter(|t| !existing_tags.iter().any(|e| &e.tag == *t))
            .map(|t| ContentTag::new(file.fingerprint.clone(), t.clone()))
            .collect();
        dao::upsert_many_content_tags(&mut tx, to_add).await?;

        tx.commit().await?;
        Ok(())
    }

    async fn get_file_tags(&self, guid: &str) -> Result<Vec<String>, Self::Error> {
        let mut conn = self.connection_pool.acquire().await?;
        let Some(file) = dao::select_file_by_guid(&mut conn, &self.user_id, guid).await? else {
            return Ok(vec![]);
        };
        let tags = dao::select_content_tags_by_fingerprint(&mut conn, &file.fingerprint).await?;
        Ok(tags.into_iter().map(|t| t.tag).collect())
    }

    async fn add_file_tags(
        &self,
        guid: &str,
        tags: Vec<String>,
    ) -> Result<Vec<String>, Self::Error> {
        self.add_file_tags_with_audit(&self.local_audit_context(), guid, tags)
            .await
    }

    async fn delete_file_tags(&self, guid: &str, tags: Vec<String>) -> Result<(), Self::Error> {
        self.delete_file_tags_with_audit(&self.local_audit_context(), guid, tags)
            .await
    }

    async fn open_file(&self, file: File) -> Result<(), Self::Error> {
        let path = match (&file.archive_path, &file.archive_inner_path) {
            (Some(archive_path), Some(inner)) => {
                extract_member_to_cache(archive_path, inner, &file.guid, &file.type_).await?
            }
            _ => std::path::PathBuf::from(&file.path),
        };
        open::that_detached(&path).map_err(|e| Error::IO(Arc::new(e)))
    }

    async fn delete_file(&self, file: File) -> Result<(), Self::Error> {
        self.delete_file_with_audit(&self.local_audit_context(), &file)
            .await
    }

    async fn get_reading_state(
        &self,
        fingerprint: &str,
    ) -> Result<Option<ReadingState>, Self::Error> {
        let mut conn = self.connection_pool.acquire().await?;
        dao::get_reading_state(&mut conn, &self.user_id, fingerprint).await
    }

    async fn upsert_reading_state(&self, state: ReadingState) -> Result<ReadingState, Self::Error> {
        let mut conn = self.connection_pool.acquire().await?;
        dao::upsert_reading_state(&mut conn, &self.user_id, state).await
    }

    async fn update_reading_status(
        &self,
        fingerprint: &str,
        status: ReadingStatus,
    ) -> Result<(), Self::Error> {
        self.update_reading_status_with_audit(&self.local_audit_context(), fingerprint, status)
            .await
    }

    async fn import_file(&self, path: &Path) -> Result<File, Self::Error> {
        let fingerprint = crate::sha256_of_file(path)
            .await
            .map_err(|e| Error::IO(Arc::new(e)))?;

        let metadata = tokio::fs::metadata(path)
            .await
            .map_err(|e| Error::IO(Arc::new(e)))?;
        let size: i32 = metadata
            .len()
            .try_into()
            .expect("file size too large for i32");

        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        let path_str = path.display().to_string();
        let mut conn = self.connection_pool.acquire().await?;

        dao::upsert_content(&mut conn, &fingerprint).await?;
        dao::upsert_file(
            &mut conn,
            NewFile {
                guid: uuid::Uuid::new_v4().to_string(),
                path: path_str.clone(),
                type_: extension,
                size,
                fingerprint: fingerprint.clone(),
                archive_path: None,
                archive_inner_path: None,
            },
        )
        .await?;

        let db_file = dao::select_file_by_path(&mut conn, &self.user_id, &path_str)
            .await?
            .expect("file should exist after upsert");
        let tags = dao::select_content_tags_by_fingerprint(&mut conn, &db_file.fingerprint).await?;
        Ok((db_file, tags).into())
    }
}

impl DbClient {
    /// A page of activity-history operations (newest first) on this database.
    pub async fn list_audit_activity(
        &self,
        limit: usize,
        cursor: Option<&dao::AuditCursor>,
    ) -> Result<crate::activity::ActivityPage, Error> {
        let page = dao::list_audit_operations(&self.connection_pool, limit, cursor).await?;
        Ok(crate::activity::ActivityPage::from(page))
    }

    /// A full activity-history operation (with its ordered events) by id.
    pub async fn get_audit_activity_operation(
        &self,
        operation_id: &str,
    ) -> Result<Option<crate::activity::ActivityDetail>, Error> {
        let pool = &self.connection_pool;
        let Some(operation) = dao::select_audit_operation(pool, operation_id).await? else {
            return Ok(None);
        };
        let events = dao::select_audit_events(pool, operation_id).await?;
        Ok(Some(crate::activity::ActivityDetail {
            operation: crate::activity::ActivityOperation::from(operation),
            events: events
                .into_iter()
                .map(crate::activity::ActivityEvent::from)
                .collect(),
        }))
    }

    /// Every operation whose events carry a target with the given document id.
    /// The document may already be gone (merged away); its history stays.
    pub async fn get_document_audit_activity(
        &self,
        document_id: &str,
    ) -> Result<Vec<crate::activity::ActivityDetail>, Error> {
        let pool = &self.connection_pool;
        let mut details = Vec::new();
        for operation_id in
            dao::find_audit_operations_by_target(pool, "document", document_id).await?
        {
            let Some(operation) = dao::select_audit_operation(pool, &operation_id).await? else {
                continue;
            };
            let events = dao::select_audit_events(pool, &operation_id).await?;
            details.push(crate::activity::ActivityDetail {
                operation: crate::activity::ActivityOperation::from(operation),
                events: events
                    .into_iter()
                    .map(crate::activity::ActivityEvent::from)
                    .collect(),
            });
        }
        Ok(details)
    }

    /// Builds an audit context for a local (non-REST) mutation performed by the
    /// embedded desktop app. REST requests never go through here — they pass
    /// their own per-request context built from the authenticated `owner` user.
    fn local_audit_context(&self) -> AuditContext {
        AuditContext::new(
            uuid::Uuid::new_v4().to_string(),
            AuditActor::local_identity(self.user_id.clone()),
            AuditChannel::Cosmic,
        )
    }

    async fn document_target(
        &self,
        conn: &mut SqliteConnection,
        fingerprint: &str,
    ) -> Option<dao::AuditTargetSnapshot> {
        let row = sqlx::query(
            "SELECT d.guid, COALESCE(NULLIF(m.title, ''), '') AS title
             FROM contents c
             JOIN documents d ON d.id = c.document_id
             LEFT JOIN document_metadata m ON m.document_id = d.id
             WHERE c.fingerprint = ?
             LIMIT 1",
        )
        .bind(fingerprint)
        .fetch_optional(&mut *conn)
        .await
        .ok()?;
        row.map(|r| {
            let guid: String = r.get("guid");
            let title: String = r.get("title");
            dao::AuditTargetSnapshot::document(guid, title)
        })
    }

    /// [`FileDataSource::add_file_tags`] with an explicit audit context. The
    /// REST layer calls this with the authenticated owner's context so tag
    /// changes are attributed to the real user, never the local identity.
    pub async fn add_file_tags_with_audit(
        &self,
        context: &AuditContext,
        guid: &str,
        tags: Vec<String>,
    ) -> Result<Vec<String>, Error> {
        if let Err(error) = dao::create_audit_operation(
            &self.connection_pool,
            context,
            OperationType::TagEdit,
            false,
            &crate::audit::now_timestamp(),
        )
        .await
        {
            tracing::warn!("could not start tag audit operation: {error}");
        }
        let mut tx = self.connection_pool.begin().await?;
        let Some(file) = dao::select_file_by_guid(&mut tx, &self.user_id, guid).await? else {
            tx.commit().await?;
            return Ok(vec![]);
        };
        let content_tags: Vec<ContentTag> = tags
            .clone()
            .into_iter()
            .map(|tag| ContentTag::new(file.fingerprint.clone(), tag))
            .collect();
        dao::upsert_many_content_tags(&mut tx, content_tags).await?;
        let event = dao::append_audit_event(
            &mut tx,
            &context.operation_id,
            AuditEventType::DocumentTagsAdded,
            AuditOutcome::Success,
            &crate::audit::now_timestamp(),
            serde_json::json!({ "tags": tags }),
            vec![dao::AuditTargetSnapshot::file(
                file.guid.clone(),
                file.path.clone(),
                file.path.clone(),
            )],
        )
        .await;
        let result = dao::select_content_tags_by_fingerprint(&mut tx, &file.fingerprint)
            .await?
            .into_iter()
            .map(|t| t.tag)
            .collect();
        tx.commit().await?;
        if let Err(error) = event {
            tracing::warn!("could not record tag-edit audit event: {error}");
        }
        if let Err(error) = dao::finish_audit_operation(
            &self.connection_pool,
            &context.operation_id,
            OperationStatus::Completed,
            &crate::audit::now_timestamp(),
            None,
            serde_json::json!({ "tags_added": tags.len() }),
        )
        .await
        {
            tracing::warn!("could not finish tag audit operation: {error}");
        }
        Ok(result)
    }

    /// [`FileDataSource::delete_file_tags`] with an explicit audit context.
    pub async fn delete_file_tags_with_audit(
        &self,
        context: &AuditContext,
        guid: &str,
        tags: Vec<String>,
    ) -> Result<(), Error> {
        if let Err(error) = dao::create_audit_operation(
            &self.connection_pool,
            context,
            OperationType::TagEdit,
            false,
            &crate::audit::now_timestamp(),
        )
        .await
        {
            tracing::warn!("could not start tag audit operation: {error}");
        }
        let mut tx = self.connection_pool.begin().await?;
        let Some(file) = dao::select_file_by_guid(&mut tx, &self.user_id, guid).await? else {
            tx.commit().await?;
            return Ok(());
        };
        let event = dao::delete_content_tags(&mut tx, &file.fingerprint, tags.clone()).await;
        if let Ok(()) = event {
            let _ = dao::append_audit_event(
                &mut tx,
                &context.operation_id,
                AuditEventType::DocumentTagsRemoved,
                AuditOutcome::Success,
                &crate::audit::now_timestamp(),
                serde_json::json!({ "tags": tags }),
                vec![dao::AuditTargetSnapshot::file(
                    file.guid.clone(),
                    file.path.clone(),
                    file.path.clone(),
                )],
            )
            .await;
        }
        tx.commit().await?;
        event?;
        if let Err(error) = dao::finish_audit_operation(
            &self.connection_pool,
            &context.operation_id,
            OperationStatus::Completed,
            &crate::audit::now_timestamp(),
            None,
            serde_json::json!({ "tags_removed": tags.len() }),
        )
        .await
        {
            tracing::warn!("could not finish tag audit operation: {error}");
        }
        Ok(())
    }

    /// [`FileDataSource::delete_file`] with an explicit audit context and
    /// truthful filesystem outcomes:
    /// * a real deletion records `file.deleted` (Success);
    /// * a file already missing on disk records `file.record_removed_missing`
    ///   with `reason: already_missing` (ReadFlow removed the record, not the
    ///   file — the observation is not attributed as the user's mutation);
    /// * an archive member removal records `file.record_removed_missing` with
    ///   `reason: archive_member`;
    /// * a filesystem error records `file.delete_failed` (Failed).
    pub async fn delete_file_with_audit(
        &self,
        context: &AuditContext,
        file: &File,
    ) -> Result<(), Error> {
        if let Err(error) = dao::create_audit_operation(
            &self.connection_pool,
            context,
            OperationType::FileDelete,
            false,
            &crate::audit::now_timestamp(),
        )
        .await
        {
            tracing::warn!("could not start delete audit operation: {error}");
        }

        let (event_type, outcome, params, op_status, error_code) = if file.archive_path.is_some() {
            (
                AuditEventType::FileRecordRemovedMissing,
                AuditOutcome::Observed,
                serde_json::json!({ "reason": "archive_member" }),
                OperationStatus::Completed,
                Some("archive_member"),
            )
        } else {
            match tokio::fs::remove_file(&file.path).await {
                Ok(()) => (
                    AuditEventType::FileDeleted,
                    AuditOutcome::Success,
                    serde_json::json!({}),
                    OperationStatus::Completed,
                    None,
                ),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (
                    AuditEventType::FileRecordRemovedMissing,
                    AuditOutcome::Observed,
                    serde_json::json!({ "reason": "already_missing" }),
                    OperationStatus::Completed,
                    Some("already_missing"),
                ),
                Err(e) => {
                    let mut err_conn = self
                        .connection_pool
                        .acquire()
                        .await
                        .unwrap_or_else(|_| panic!("database available"));
                    let _ = dao::append_audit_event(
                        &mut err_conn,
                        &context.operation_id,
                        AuditEventType::FileDeleteFailed,
                        AuditOutcome::Failed,
                        &crate::audit::now_timestamp(),
                        serde_json::json!({ "reason": e.kind().to_string() }),
                        vec![dao::AuditTargetSnapshot::file(
                            file.guid.clone(),
                            file.path.clone(),
                            file.path.clone(),
                        )],
                    )
                    .await;
                    if let Err(finish_err) = dao::finish_audit_operation(
                        &self.connection_pool,
                        &context.operation_id,
                        OperationStatus::Failed,
                        &crate::audit::now_timestamp(),
                        Some("filesystem_error"),
                        serde_json::json!({}),
                    )
                    .await
                    {
                        tracing::warn!("could not finish delete audit operation: {finish_err}");
                    }
                    tracing::warn!("Failed to delete file from filesystem: {}", e);
                    return Err(Error::IO(Arc::new(e)));
                }
            }
        };

        let mut tx = self.connection_pool.begin().await?;
        let mut error = None;
        if let Some(db_file) = dao::select_file_by_guid(&mut tx, &self.user_id, &file.guid).await? {
            if let Err(e) = dao::delete_file_record(&mut tx, db_file.id).await {
                error = Some(e);
            }
        }
        if error.is_none() {
            let _ = dao::append_audit_event(
                &mut tx,
                &context.operation_id,
                event_type,
                outcome,
                &crate::audit::now_timestamp(),
                params,
                vec![dao::AuditTargetSnapshot::file(
                    file.guid.clone(),
                    file.path.clone(),
                    file.path.clone(),
                )],
            )
            .await;
        }
        if let Err(finish_error) = dao::finish_audit_operation_on_connection(
            &mut tx,
            &context.operation_id,
            op_status,
            &crate::audit::now_timestamp(),
            error_code,
            serde_json::json!({}),
        )
        .await
        {
            tracing::warn!("could not finish delete audit operation: {finish_error}");
        }
        tx.commit().await?;
        match error {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// [`Self::update_reading_status`] with an explicit audit context, targeted
    /// at the document (or the file's path when no document is linked yet).
    /// The reading state is updated for this client's [`Self::user_id`] — pass
    /// [`Self::update_reading_status_for_user_with_audit`] to target another
    /// user's state instead (e.g. the authenticated owner of a REST request).
    pub async fn update_reading_status_with_audit(
        &self,
        context: &AuditContext,
        fingerprint: &str,
        status: ReadingStatus,
    ) -> Result<(), Error> {
        self.update_reading_status_for_user_with_audit(context, &self.user_id, fingerprint, status)
            .await
    }

    /// [`Self::update_reading_status_with_audit`] but updates the reading
    /// state of `user_id` (the REST request owner, say) instead of this
    /// client's local user. The audit event still records `context`'s actor.
    pub async fn update_reading_status_for_user_with_audit(
        &self,
        context: &AuditContext,
        user_id: &str,
        fingerprint: &str,
        status: ReadingStatus,
    ) -> Result<(), Error> {
        if let Err(error) = dao::create_audit_operation(
            &self.connection_pool,
            context,
            OperationType::StatusEdit,
            false,
            &crate::audit::now_timestamp(),
        )
        .await
        {
            tracing::warn!("could not start status audit operation: {error}");
        }
        let mut tx = self.connection_pool.begin().await?;
        let event =
            dao::update_reading_status_only(&mut tx, user_id, fingerprint, status.into()).await;
        let target = self
            .document_target(&mut tx, fingerprint)
            .await
            .unwrap_or_else(|| dao::AuditTargetSnapshot::path(fingerprint.to_string()));
        if let Ok(()) = event {
            let _ = dao::append_audit_event(
                &mut tx,
                &context.operation_id,
                AuditEventType::DocumentStatusChanged,
                AuditOutcome::Success,
                &crate::audit::now_timestamp(),
                serde_json::json!({ "status": status_str(status) }),
                vec![target],
            )
            .await;
        }
        let finish = dao::finish_audit_operation_on_connection(
            &mut tx,
            &context.operation_id,
            OperationStatus::Completed,
            &crate::audit::now_timestamp(),
            None,
            serde_json::json!({ "status": status_str(status) }),
        )
        .await;
        tx.commit().await?;
        if let Err(e) = finish {
            tracing::warn!("could not finish status audit operation: {e}");
        }
        event
    }

    /// [`Self::update_document_metadata`] with an explicit audit context. Also
    /// records `document.cover_changed` inside the same operation when the
    /// selected cover changed.
    pub async fn update_document_metadata_with_audit(
        &self,
        context: &AuditContext,
        guid: &str,
        meta: DocumentMeta,
    ) -> Result<Option<ApiDocument>, Error> {
        self.update_document_metadata_impl(Some(context), guid, meta)
            .await
    }

    /// [`Self::update_document_metadata`] without audit (local trait-free uses;
    /// kept for callers that manage their own history, as the API's bare getter).
    pub async fn update_document_metadata(
        &self,
        guid: &str,
        meta: DocumentMeta,
    ) -> Result<Option<ApiDocument>, Error> {
        self.update_document_metadata_impl(None, guid, meta).await
    }

    async fn update_document_metadata_impl(
        &self,
        context: Option<&AuditContext>,
        guid: &str,
        meta: DocumentMeta,
    ) -> Result<Option<ApiDocument>, Error> {
        if let Some(context) = context {
            if let Err(error) = dao::create_audit_operation(
                &self.connection_pool,
                context,
                OperationType::MetadataEdit,
                false,
                &crate::audit::now_timestamp(),
            )
            .await
            {
                tracing::warn!("could not start metadata audit operation: {error}");
            }
        }
        let mut conn = self.connection_pool.acquire().await?;
        let Some(doc_row) = dao::select_document_by_guid(&mut conn, guid).await? else {
            return Ok(None);
        };
        let previous_cover = dao::get_document_user_metadata(&mut conn, doc_row.id)
            .await?
            .and_then(|metadata| metadata.selected_cover_fingerprint);
        let doc_type_str = meta.document_type_str();
        let authors_json = meta.authors_json();
        dao::upsert_document_user_metadata(
            &mut conn,
            doc_row.id,
            doc_type_str.as_deref(),
            meta.title.as_deref(),
            meta.subtitle.as_deref(),
            authors_json.as_deref(),
            meta.description.as_deref(),
            meta.language.as_deref(),
            meta.publisher.as_deref(),
            meta.identifier.as_deref(),
            meta.date.as_deref(),
            meta.subject.as_deref(),
            meta.selected_cover_fingerprint.as_deref(),
        )
        .await?;
        let result = dao::select_api_document_by_guid(&mut conn, guid).await?;

        if let Some(context) = context {
            let title = result
                .as_ref()
                .map(|doc| doc.metadata.title.clone().unwrap_or_default())
                .unwrap_or_default();
            let target = dao::AuditTargetSnapshot::document(guid.to_string(), title);
            if let Err(error) = dao::append_audit_event(
                &mut conn,
                &context.operation_id,
                AuditEventType::DocumentMetadataChanged,
                AuditOutcome::Success,
                &crate::audit::now_timestamp(),
                serde_json::json!({}),
                vec![target.clone()],
            )
            .await
            {
                tracing::warn!("could not record metadata audit event: {error}");
            }
            if previous_cover != meta.selected_cover_fingerprint {
                let _ = dao::append_audit_event(
                    &mut conn,
                    &context.operation_id,
                    AuditEventType::DocumentCoverChanged,
                    AuditOutcome::Success,
                    &crate::audit::now_timestamp(),
                    serde_json::json!({}),
                    vec![target],
                )
                .await;
            }
            if let Err(error) = dao::finish_audit_operation(
                &self.connection_pool,
                &context.operation_id,
                OperationStatus::Completed,
                &crate::audit::now_timestamp(),
                None,
                serde_json::json!({}),
            )
            .await
            {
                tracing::warn!("could not finish metadata audit operation: {error}");
            }
        }
        Ok(result)
    }

    /// [`Self::store_cover`] with an explicit audit context.
    pub async fn store_cover_with_audit(
        &self,
        context: &AuditContext,
        fingerprint: &str,
        data: &[u8],
        mime: &str,
    ) -> Result<(), Error> {
        if let Err(error) = dao::create_audit_operation(
            &self.connection_pool,
            context,
            OperationType::CoverEdit,
            false,
            &crate::audit::now_timestamp(),
        )
        .await
        {
            tracing::warn!("could not start cover audit operation: {error}");
        }
        let mut tx = self.connection_pool.begin().await?;
        let target = self
            .document_target(&mut tx, fingerprint)
            .await
            .unwrap_or_else(|| dao::AuditTargetSnapshot::path(fingerprint.to_string()));
        let result = dao::upsert_cover(&mut tx, fingerprint, data, mime).await;
        if let Ok(()) = result {
            let _ = dao::append_audit_event(
                &mut tx,
                &context.operation_id,
                AuditEventType::DocumentCoverChanged,
                AuditOutcome::Success,
                &crate::audit::now_timestamp(),
                serde_json::json!({}),
                vec![target],
            )
            .await;
        }
        let finish = dao::finish_audit_operation_on_connection(
            &mut tx,
            &context.operation_id,
            OperationStatus::Completed,
            &crate::audit::now_timestamp(),
            None,
            serde_json::json!({}),
        )
        .await;
        tx.commit().await?;
        if let Err(error) = finish {
            tracing::warn!("could not finish cover audit operation: {error}");
        }
        result
    }

    /// [`Self::import_file`] guarded by an explicit audit context.
    pub async fn import_file_with_audit(
        &self,
        context: &AuditContext,
        path: &Path,
    ) -> Result<File, Error> {
        if let Err(error) = dao::create_audit_operation(
            &self.connection_pool,
            context,
            OperationType::Scan,
            false,
            &crate::audit::now_timestamp(),
        )
        .await
        {
            tracing::warn!("could not start import audit operation: {error}");
        }
        let file = self.import_file(path).await;
        let mut audit_conn = self.connection_pool.acquire().await?;
        let (status, counts) = match &file {
            Ok(file) => {
                let _ = dao::append_audit_event(
                    &mut audit_conn,
                    &context.operation_id,
                    AuditEventType::ScanFileDiscovered,
                    AuditOutcome::Success,
                    &crate::audit::now_timestamp(),
                    serde_json::json!({ "path": file.path }),
                    vec![dao::AuditTargetSnapshot::file(
                        file.guid.clone(),
                        file.path.clone(),
                        file.path.clone(),
                    )],
                )
                .await;
                (
                    OperationStatus::Completed,
                    serde_json::json!({ "imported": 1 }),
                )
            }
            Err(_) => (OperationStatus::Failed, serde_json::json!({})),
        };
        drop(audit_conn);
        if let Err(error) = dao::finish_audit_operation(
            &self.connection_pool,
            &context.operation_id,
            status,
            &crate::audit::now_timestamp(),
            None,
            counts,
        )
        .await
        {
            tracing::warn!("could not finish import audit operation: {error}");
        }
        file
    }

    /// [`Self::merge_documents`] with an explicit audit context.
    pub async fn merge_documents_with_audit(
        &self,
        context: &AuditContext,
        winner_guid: &str,
        loser_guids: &[String],
    ) -> Result<(), Error> {
        dao::merge_documents_with_audit(&self.connection_pool, context, winner_guid, loser_guids)
            .await
    }

    pub async fn store_cover(
        &self,
        fingerprint: &str,
        data: &[u8],
        mime: &str,
    ) -> Result<(), Error> {
        let mut conn = self.connection_pool.acquire().await?;
        dao::upsert_cover(&mut conn, fingerprint, data, mime).await
    }

    /// Import a file and immediately apply OPDS-sourced metadata to the document.
    /// Combines `import_file`, document-creation, and metadata merge in one call.
    pub async fn import_with_opds_metadata(
        &self,
        path: &Path,
        meta: &ExtractedMetadata,
    ) -> Result<File, Error> {
        let file = self.import_file(path).await?;
        if !meta.is_empty() {
            let mut conn = self.connection_pool.acquire().await?;
            match dao::ensure_document_for_fingerprint(&mut conn, &file.fingerprint).await {
                Ok(api_doc) => {
                    let doc_id_result =
                        sqlx::query_scalar::<_, i32>("SELECT id FROM documents WHERE guid = ?")
                            .bind(&api_doc.guid)
                            .fetch_one(&mut *conn)
                            .await;
                    match doc_id_result {
                        Ok(doc_id) => {
                            if let Err(e) =
                                dao::merge_document_metadata_from_extracted(&mut conn, doc_id, meta)
                                    .await
                            {
                                tracing::warn!(
                                    "failed to apply OPDS metadata for {}: {e}",
                                    file.fingerprint
                                );
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                "failed to resolve document id for {}: {e}",
                                file.fingerprint
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("failed to ensure document for {}: {e}", file.fingerprint);
                }
            }
        }
        Ok(file)
    }

    pub async fn get_documents(&self) -> Result<Vec<ApiDocument>, Error> {
        let mut conn = self.connection_pool.acquire().await?;
        dao::select_all_api_documents(&mut conn).await
    }

    pub async fn get_document(&self, guid: &str) -> Result<Option<ApiDocument>, Error> {
        let mut conn = self.connection_pool.acquire().await?;
        dao::select_api_document_by_guid(&mut conn, guid).await
    }

    pub async fn ensure_document_for_file(&self, file_guid: &str) -> Result<ApiDocument, Error> {
        let mut conn = self.connection_pool.acquire().await?;
        dao::ensure_document_for_file_guid(&mut conn, &self.user_id, file_guid).await
    }

    pub async fn merge_documents(
        &self,
        winner_guid: &str,
        loser_guids: &[String],
    ) -> Result<(), Error> {
        self.merge_documents_with_audit(&self.local_audit_context(), winner_guid, loser_guids)
            .await
    }

    /// Remove a fingerprint and all its files from a document (purge).
    pub async fn delete_content_from_document_with_audit(
        &self,
        context: &AuditContext,
        document_guid: &str,
        fingerprint: &str,
    ) -> Result<Option<dao::DeleteContentResult>, Error> {
        dao::delete_content_from_document(
            &self.connection_pool,
            Some(context),
            document_guid,
            fingerprint,
        )
        .await
    }

    /// [`Self::delete_content_from_document_with_audit`] with a local audit context.
    pub async fn delete_content_from_document(
        &self,
        document_guid: &str,
        fingerprint: &str,
    ) -> Result<Option<dao::DeleteContentResult>, Error> {
        dao::delete_content_from_document(
            &self.connection_pool,
            Some(&self.local_audit_context()),
            document_guid,
            fingerprint,
        )
        .await
    }

    /// Resolve a readable filesystem path for a file, extracting archive
    /// members to a temp file when needed. Drop the returned guard only once
    /// the path is no longer read — it deletes the temp file.
    async fn resolve_pdf_file(
        &self,
        file_guid: &str,
    ) -> Result<
        (
            crate::db::models::File,
            std::path::PathBuf,
            Option<tempfile::NamedTempFile>,
        ),
        Error,
    > {
        let mut conn = self.connection_pool.acquire().await?;
        let file = dao::select_file_by_guid(&mut conn, &self.user_id, file_guid)
            .await?
            .ok_or_else(|| {
                Error::IO(Arc::new(std::io::Error::other(format!(
                    "file not found: {file_guid}"
                ))))
            })?;
        if file.type_.to_lowercase() != "pdf" {
            return Err(Error::IO(Arc::new(std::io::Error::other(format!(
                "file type {} is not a PDF",
                file.type_
            )))));
        }
        let (path, guard) = match (&file.archive_path, &file.archive_inner_path) {
            (Some(archive_path), Some(inner)) => {
                let tmp = crate::scan::scanner::extract_member_to_temp_file(
                    std::path::PathBuf::from(archive_path),
                    inner.clone(),
                    file.type_.clone(),
                )
                .await?;
                let path = tmp.path().to_path_buf();
                (path, Some(tmp))
            }
            _ => (std::path::PathBuf::from(&file.path), None),
        };
        Ok((file, path, guard))
    }

    /// @feature: documents.change_thumbnail
    pub async fn get_pdf_page_count(&self, file_guid: &str) -> Result<i32, Error> {
        let (_file, path, _guard) = self.resolve_pdf_file(file_guid).await?;
        tokio::task::spawn_blocking(move || crate::scan::cover::pdf_page_count(&path))
            .await
            .map_err(|e| Error::IO(Arc::new(std::io::Error::other(e))))?
            .ok_or_else(|| {
                Error::IO(Arc::new(std::io::Error::other(
                    "could not read PDF page count",
                )))
            })
    }

    /// @feature: documents.change_thumbnail
    pub async fn get_pdf_page_preview(
        &self,
        file_guid: &str,
        page_index: i32,
        trim: bool,
        padding: u32,
        margins: crate::scan::cover::TrimMargins,
        thumb: bool,
    ) -> Result<Vec<u8>, Error> {
        let (_file, path, _guard) = self.resolve_pdf_file(file_guid).await?;
        let max_dim = if thumb { 200 } else { 800 };
        tokio::task::spawn_blocking(move || {
            let img = crate::scan::cover::render_pdf_page(&path, page_index, max_dim)?;
            let img = if trim {
                crate::scan::cover::trim_whitespace(&img, padding, margins)
            } else {
                img
            };
            crate::scan::cover::encode_page_webp(&img)
        })
        .await
        .map_err(|e| Error::IO(Arc::new(std::io::Error::other(e))))?
        .ok_or_else(|| Error::IO(Arc::new(std::io::Error::other("could not render page"))))
    }

    /// @feature: documents.change_thumbnail
    pub async fn set_pdf_page_thumbnail(
        &self,
        file_guid: &str,
        page_index: i32,
        trim: bool,
        padding: u32,
        margins: crate::scan::cover::TrimMargins,
    ) -> Result<ApiDocument, Error> {
        let (file, path, _guard) = self.resolve_pdf_file(file_guid).await?;
        let (data, mime) = tokio::task::spawn_blocking(move || {
            let img = crate::scan::cover::render_pdf_page(&path, page_index, 800)?;
            let img = if trim {
                crate::scan::cover::trim_whitespace(&img, padding, margins)
            } else {
                img
            };
            crate::scan::cover::encode_page_webp(&img).map(|data| (data, "image/webp".to_string()))
        })
        .await
        .map_err(|e| Error::IO(Arc::new(std::io::Error::other(e))))?
        .ok_or_else(|| Error::IO(Arc::new(std::io::Error::other("could not render page"))))?;

        let mut conn = self.connection_pool.acquire().await?;
        dao::set_custom_cover(
            &mut conn,
            &file.fingerprint,
            page_index as i64,
            trim,
            &data,
            &mime,
        )
        .await?;
        let doc = dao::ensure_document_for_fingerprint(&mut conn, &file.fingerprint).await?;
        let doc_row = dao::select_document_by_guid(&mut conn, &doc.guid)
            .await?
            .ok_or_else(|| {
                Error::IO(Arc::new(std::io::Error::other(
                    "document must exist after ensure_document_for_fingerprint",
                )))
            })?;
        dao::set_selected_cover_fingerprint(&mut conn, doc_row.id, &file.fingerprint).await?;
        dao::select_api_document_by_guid(&mut conn, &doc.guid)
            .await?
            .ok_or_else(|| {
                Error::IO(Arc::new(std::io::Error::other(
                    "document must exist after upsert",
                )))
            })
    }
}

/// Wraps a [`DbClient`] and filters out files/tags whose tags include any of
/// the configured hidden (private) tags. Constructed via
/// [`ApplicationModule::filtered_db_client`].
#[derive(Clone)]
pub struct FilteredDbClient {
    inner: DbClient,
    hidden_tags: Vec<String>,
}

impl FilteredDbClient {
    pub fn new(inner: DbClient, hidden_tags: Vec<String>) -> Self {
        Self { inner, hidden_tags }
    }
}

#[async_trait::async_trait]
impl FileDataSource for FilteredDbClient {
    type Error = Error;

    fn display_name(&self) -> String {
        self.inner.display_name()
    }

    async fn status(&self) -> Result<Status, Self::Error> {
        self.inner.status().await
    }

    async fn get_files(&self) -> Result<Vec<File>, Self::Error> {
        let files = self.inner.get_files().await?;
        Ok(files
            .into_iter()
            .filter(|f| !f.tags.iter().any(|t| self.hidden_tags.contains(t)))
            .collect())
    }

    async fn get_files_tags(&self) -> Result<Vec<String>, Self::Error> {
        let tags = self.inner.get_files_tags().await?;
        Ok(tags
            .into_iter()
            .filter(|t| !self.hidden_tags.contains(t))
            .collect())
    }

    async fn get_file(&self, guid: &str) -> Result<Option<File>, Self::Error> {
        self.inner.get_file(guid).await
    }

    async fn get_file_tags(&self, guid: &str) -> Result<Vec<String>, Self::Error> {
        self.inner.get_file_tags(guid).await
    }

    async fn add_file_tags(
        &self,
        guid: &str,
        tags: Vec<String>,
    ) -> Result<Vec<String>, Self::Error> {
        self.inner.add_file_tags(guid, tags).await
    }

    async fn delete_file_tags(&self, guid: &str, tags: Vec<String>) -> Result<(), Self::Error> {
        self.inner.delete_file_tags(guid, tags).await
    }

    async fn update_file(&self, file: File) -> Result<(), Self::Error> {
        self.inner.update_file(file).await
    }

    async fn open_file(&self, file: File) -> Result<(), Self::Error> {
        self.inner.open_file(file).await
    }

    async fn delete_file(&self, file: File) -> Result<(), Self::Error> {
        self.inner.delete_file(file).await
    }

    async fn import_file(&self, path: &Path) -> Result<File, Self::Error> {
        self.inner.import_file(path).await
    }

    async fn get_reading_state(
        &self,
        fingerprint: &str,
    ) -> Result<Option<ReadingState>, Self::Error> {
        self.inner.get_reading_state(fingerprint).await
    }

    async fn upsert_reading_state(&self, state: ReadingState) -> Result<ReadingState, Self::Error> {
        self.inner.upsert_reading_state(state).await
    }

    async fn update_reading_status(
        &self,
        fingerprint: &str,
        status: ReadingStatus,
    ) -> Result<(), Self::Error> {
        self.inner.update_reading_status(fingerprint, status).await
    }
}

/// Stable lowercase wire value for a reading status (used in audit parameters).
fn status_str(status: ReadingStatus) -> &'static str {
    match status {
        ReadingStatus::Unread => "unread",
        ReadingStatus::Reading => "reading",
        ReadingStatus::Read => "read",
    }
}
