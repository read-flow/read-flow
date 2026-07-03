use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

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
}

impl DbClient {
    pub fn new(connection_pool: ConnectionPool) -> Self {
        Self { connection_pool }
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
        let files = dao::select_all_files(&mut conn).await?;
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
        let Some(file) = dao::select_file_by_guid(&mut conn, guid).await? else {
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

        let Some(existing) = dao::select_file_by_guid(&mut tx, &file.guid).await? else {
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
        let Some(file) = dao::select_file_by_guid(&mut conn, guid).await? else {
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
        let mut conn = self.connection_pool.acquire().await?;
        let Some(file) = dao::select_file_by_guid(&mut conn, guid).await? else {
            return Ok(vec![]);
        };
        let content_tags: Vec<ContentTag> = tags
            .into_iter()
            .map(|tag| ContentTag::new(file.fingerprint.clone(), tag))
            .collect();
        dao::upsert_many_content_tags(&mut conn, content_tags).await?;
        let result = dao::select_content_tags_by_fingerprint(&mut conn, &file.fingerprint)
            .await?
            .into_iter()
            .map(|t| t.tag)
            .collect();
        Ok(result)
    }

    async fn delete_file_tags(&self, guid: &str, tags: Vec<String>) -> Result<(), Self::Error> {
        let mut conn = self.connection_pool.acquire().await?;
        let Some(file) = dao::select_file_by_guid(&mut conn, guid).await? else {
            return Ok(());
        };
        dao::delete_content_tags(&mut conn, &file.fingerprint, tags).await
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
        if let Err(e) = tokio::fs::remove_file(&file.path).await {
            tracing::warn!("Failed to delete file from filesystem: {}", e);
            return Err(Error::IO(Arc::new(e)));
        }
        let mut conn = self.connection_pool.acquire().await?;
        if let Some(db_file) = dao::select_file_by_guid(&mut conn, &file.guid).await? {
            dao::delete_file_record(&self.connection_pool, db_file.id).await?;
        }
        Ok(())
    }

    async fn get_reading_state(
        &self,
        fingerprint: &str,
    ) -> Result<Option<ReadingState>, Self::Error> {
        let mut conn = self.connection_pool.acquire().await?;
        dao::get_reading_state(&mut conn, fingerprint).await
    }

    async fn upsert_reading_state(&self, state: ReadingState) -> Result<ReadingState, Self::Error> {
        let mut conn = self.connection_pool.acquire().await?;
        dao::upsert_reading_state(&mut conn, state).await
    }

    async fn update_reading_status(
        &self,
        fingerprint: &str,
        status: ReadingStatus,
    ) -> Result<(), Self::Error> {
        let mut conn = self.connection_pool.acquire().await?;
        dao::update_reading_status_only(&mut conn, fingerprint, status.into()).await
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

        let db_file = dao::select_file_by_path(&mut conn, &path_str)
            .await?
            .expect("file should exist after upsert");
        let tags = dao::select_content_tags_by_fingerprint(&mut conn, &db_file.fingerprint).await?;
        Ok((db_file, tags).into())
    }
}

impl DbClient {
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

    pub async fn store_cover(
        &self,
        fingerprint: &str,
        data: &[u8],
        mime: &str,
    ) -> Result<(), Error> {
        let mut conn = self.connection_pool.acquire().await?;
        dao::upsert_cover(&mut conn, fingerprint, data, mime).await
    }

    pub async fn get_documents(&self) -> Result<Vec<ApiDocument>, Error> {
        let mut conn = self.connection_pool.acquire().await?;
        dao::select_all_api_documents(&mut conn).await
    }

    pub async fn get_document(&self, guid: &str) -> Result<Option<ApiDocument>, Error> {
        let mut conn = self.connection_pool.acquire().await?;
        dao::select_api_document_by_guid(&mut conn, guid).await
    }

    pub async fn update_document_metadata(
        &self,
        guid: &str,
        meta: DocumentMeta,
    ) -> Result<Option<ApiDocument>, Error> {
        let mut conn = self.connection_pool.acquire().await?;
        let Some(doc_row) = dao::select_document_by_guid(&mut conn, guid).await? else {
            return Ok(None);
        };
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
        dao::select_api_document_by_guid(&mut conn, guid).await
    }

    pub async fn ensure_document_for_file(&self, file_guid: &str) -> Result<ApiDocument, Error> {
        let mut conn = self.connection_pool.acquire().await?;
        dao::ensure_document_for_file_guid(&mut conn, file_guid).await
    }

    pub async fn merge_documents(
        &self,
        winner_guid: &str,
        loser_guids: &[String],
    ) -> Result<(), Error> {
        dao::merge_documents(&self.connection_pool, winner_guid, loser_guids).await
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
