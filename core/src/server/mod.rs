mod authn;

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use authn::AuthorizedUser;
use authn::PrivateModeHeader;
use axum::Json;
use axum::Router;
use axum::extract::Multipart;
use axum::extract::Path as AxumPath;
use axum::extract::Query;
use axum::extract::State;
use axum::http::StatusCode;
use axum::http::header;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::routing::get;
use axum::routing::post;
use axum::routing::put;
use figment::Figment;
use provider::r#async::AndThen;
use provider::r#async::Provider;
use tokio::net::TcpListener;
use tower_http::cors::Any;
use tower_http::cors::CorsLayer;

use crate::ApplicationModule;
use crate::ExpandedPath;
use crate::api::ApiDocument;
use crate::api::DocumentMeta;
use crate::api::File;
use crate::api::FileDataSource;
use crate::api::MergeDocumentsRequest;
use crate::api::ReadingState;
use crate::api::ReadingStatus;
use crate::api::Status;
use crate::db::ConnectionPool;
use crate::db::dao;
use crate::db::datasource::DbClient;
use crate::online_library::DownloadFormat;
use crate::online_library::OnlineBook;
use crate::online_library::OnlineCatalog;
use crate::online_library::OpdsClient;
use crate::online_library::download_book;
use crate::scan::DirectorySettings;
use crate::scan::DocumentType;
use crate::scan::ScanSummary;
use crate::settings;
use crate::settings::HashedPassword;
pub use crate::settings::ServerSettings;
use crate::settings::Settings;
use crate::settings::SettingsError;
use crate::settings::UserEntry;
use crate::to_unique_file;

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("database error: {0}")]
    Dao(String, #[source] dao::Error),
    #[error("filesystem error: {0}")]
    Io(#[from] io::Error),
    #[error("extension {0} is not supported")]
    UnsupportedExtension(String),
    #[error("content-type {0} is not supported")]
    UnsupportedContentType(String),
    #[error("could not import file: {0}")]
    Scan(String),
    #[error("file with guid {0} not found")]
    FileNotFound(String),
    #[error("private mode access requires owner role")]
    Forbidden(String),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("settings error: {0}")]
    Settings(String),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status = match &self {
            Error::Dao(..) | Error::Io(_) | Error::Scan(_) | Error::Settings(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            Error::UnsupportedExtension(_)
            | Error::UnsupportedContentType(_)
            | Error::BadRequest(_) => StatusCode::BAD_REQUEST,
            Error::FileNotFound(_) => StatusCode::NOT_FOUND,
            Error::Forbidden(_) => StatusCode::FORBIDDEN,
        };
        (status, self.to_string()).into_response()
    }
}

impl From<SettingsError> for Error {
    fn from(error: SettingsError) -> Self {
        tracing::error!("settings error: {error}");
        Error::Settings(error.to_string())
    }
}

impl From<dao::Error> for Error {
    fn from(error: dao::Error) -> Self {
        tracing::error!("database error: {error}");
        Error::Dao(error.to_string(), error)
    }
}

impl From<anyhow::Error> for Error {
    fn from(error: anyhow::Error) -> Self {
        tracing::error!("could not import file: {error}");
        Error::Scan(error.to_string())
    }
}

type Result<T, E = Error> = std::result::Result<T, E>;

/// The subset of [`ApplicationModule`] behaviour the HTTP handlers and
/// extractors depend on. Object-safe, so the server can run over **any**
/// settings provider `P` (e.g. the COSMIC app's own `ApplicationModule`)
/// without making every handler generic — the app shares one module with the
/// embedded server.
#[async_trait::async_trait]
pub trait ServerModule: Send + Sync + 'static {
    async fn settings(&self) -> Settings;
    async fn connection_pool(&self) -> ConnectionPool;
    async fn db_client(&self) -> DbClient;
    async fn scan(&self, path: PathBuf) -> anyhow::Result<()>;
    async fn scan_configured(&self) -> anyhow::Result<ScanSummary>;
    async fn check_missing(&self, purge: bool) -> Vec<String>;
    async fn update_settings(
        &self,
        mutate: Box<dyn for<'a> FnOnce(&'a mut Settings) + Send>,
    ) -> std::result::Result<(), SettingsError>;
    async fn reload_settings(&self);
}

#[async_trait::async_trait]
impl<P> ServerModule for ApplicationModule<P>
where
    P: Provider<Settings, Error = SettingsError> + Send + Sync + 'static,
{
    // NB: `self` is the concrete `ApplicationModule<P>`, so these resolve to the
    // inherent methods (inherent methods shadow trait methods of the same name),
    // not back into this trait impl.
    async fn settings(&self) -> Settings {
        self.settings().await
    }
    async fn connection_pool(&self) -> ConnectionPool {
        self.connection_pool().await
    }
    async fn db_client(&self) -> DbClient {
        self.db_client().await
    }
    async fn scan(&self, path: PathBuf) -> anyhow::Result<()> {
        self.scan(path).await
    }
    async fn scan_configured(&self) -> anyhow::Result<ScanSummary> {
        self.scan_configured().await
    }
    async fn check_missing(&self, purge: bool) -> Vec<String> {
        self.check_missing(purge).await
    }
    async fn update_settings(
        &self,
        mutate: Box<dyn for<'a> FnOnce(&'a mut Settings) + Send>,
    ) -> std::result::Result<(), SettingsError> {
        self.update_settings(mutate).await
    }
    async fn reload_settings(&self) {
        self.reload_settings().await
    }
}

/// Shared application state handed to every handler and extractor. Cheap to
/// clone (`Arc`), derefs to a [`ServerModule`] so handler bodies read the same
/// as before.
#[derive(Clone)]
pub struct AppState(Arc<dyn ServerModule>);

impl AppState {
    pub fn new(module: Arc<dyn ServerModule>) -> Self {
        Self(module)
    }
}

impl std::ops::Deref for AppState {
    type Target = dyn ServerModule;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

/// Permissive CORS policy mirroring the previous `rocket_cors` setup: any
/// origin, the same method set, and any header.
fn cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
}

pub struct FigmentProvider {
    config_path: PathBuf,
}

impl Provider<Figment> for FigmentProvider {
    type Error = SettingsError;
    async fn provide(&self) -> Result<Figment, Self::Error> {
        Ok(settings::decorate_with(
            Figment::new(),
            self.config_path.clone(),
        ))
    }
}

pub type SettingsProvider =
    AndThen<FigmentProvider, fn(Figment) -> Result<Settings, SettingsError>, Figment>;

fn extract_settings(figment: Figment) -> Result<Settings, SettingsError> {
    Ok(figment.extract()?)
}

async fn build_state(config_path: PathBuf) -> anyhow::Result<AppState> {
    let figment_provider = FigmentProvider {
        config_path: config_path.clone(),
    };
    let settings_provider = figment_provider
        .and_then(extract_settings as fn(Figment) -> Result<Settings, SettingsError>);

    let application_module: ApplicationModule<SettingsProvider> =
        ApplicationModule::new(settings_provider, config_path).await?;

    Ok(AppState::new(Arc::new(application_module)))
}

/// Build the fully-configured router (routes + CORS + state). Exposed so the
/// COSMIC app can embed the server in-process and serve it on its own runtime.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/status", get(status))
        .route("/files", get(get_files).put(update_file).post(upload_file))
        .route("/files/tags", get(get_files_tags))
        .route("/files/{guid}", get(get_file).delete(delete_file))
        .route(
            "/files/{guid}/tags",
            get(get_file_tags)
                .post(post_file_tags)
                .delete(delete_file_tags),
        )
        .route("/files/{guid}/download-as/{file_name}", get(download_file))
        .route("/files/{guid}/cover", get(get_file_cover))
        .route("/files/{guid}/document", post(ensure_document_for_file))
        .route("/reading-state", put(put_reading_state))
        .route("/reading-state/{fingerprint}", get(get_reading_state))
        .route(
            "/reading-state/{fingerprint}/status",
            put(put_reading_status),
        )
        .route("/documents", get(get_documents))
        .route("/documents/merge", post(post_merge_documents))
        .route("/documents/{guid}", get(get_document))
        .route("/documents/{guid}/cover", get(get_document_cover))
        .route("/documents/{guid}/metadata", put(put_document_metadata))
        .route("/scan", post(post_scan))
        .route("/maintenance/check-missing", post(post_check_missing))
        .route(
            "/scan-directories",
            get(get_scan_directories)
                .put(put_scan_directory)
                .delete(delete_scan_directory),
        )
        .route("/settings", get(get_settings).put(put_settings))
        .route("/users", get(get_users).post(post_user))
        .route("/users/{user_id}", put(put_user).delete(delete_user))
        .route("/online-library/search", get(search_online_library))
        .route("/online-library/import", post(import_online_book))
        .layer(cors_layer())
        .with_state(state)
}

/// Build the router directly from a configuration file. Convenience entry point
/// for embedding the server.
pub async fn build_app(config_path: PathBuf) -> anyhow::Result<Router> {
    Ok(build_router(build_state(config_path).await?))
}

/// Serve an already-built router on the given listener until shutdown.
pub async fn serve_on(listener: TcpListener, app: Router) -> std::io::Result<()> {
    axum::serve(listener, app).await
}

/// Serve until either the process ends or `shutdown` resolves. The shutdown
/// hook is how the embedding app (COSMIC) stops/restarts the server: complete
/// the future and `axum` drains in-flight requests and returns.
pub async fn serve_on_with_shutdown(
    listener: TcpListener,
    app: Router,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
}

pub async fn main(config_path: PathBuf) -> anyhow::Result<()> {
    let state = build_state(config_path).await?;
    let addr = state.settings().await.server.bind_addr();
    let listener = TcpListener::bind(addr).await?;
    // Printed to stdout (tracing goes to stderr) so test/e2e harnesses can
    // parse the bound address, which matters when `port = 0`.
    println!("Server listening on http://{}", listener.local_addr()?);
    let app = build_router(state);
    serve_on(listener, app).await?;
    Ok(())
}

/// @feature: remotes.status
async fn status(
    State(application_module): State<AppState>,
    user: AuthorizedUser,
) -> Result<Json<Status>> {
    let db_status = application_module.db_client().await.status().await?;
    let status = Status {
        identifier: "server".to_string(),
        attributes: HashMap::from_iter([("user_id".to_string(), user.user_id)]),
        nested_checks: vec![db_status],
    };
    Ok(Json(status))
}

async fn get_files(
    State(application_module): State<AppState>,
    user: AuthorizedUser,
    private_mode: PrivateModeHeader,
) -> Result<Json<Vec<File>>> {
    if private_mode.0 {
        if !user.has_role("owner") {
            return Err(Error::Forbidden(
                "private mode access requires owner role".into(),
            ));
        }
        let files = application_module.db_client().await.get_files().await?;
        return Ok(Json(files));
    }

    let settings = application_module.settings().await;
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let excluded = settings.ui.private_tags().to_vec();
    let db_files = dao::select_all_files_excluding_tags(&mut conn, &excluded).await?;
    let all_tags = dao::select_all_content_tags(&mut conn).await?;
    let mut tags_by_fp: std::collections::HashMap<String, Vec<crate::db::models::ContentTag>> =
        std::collections::HashMap::new();
    for tag in all_tags {
        tags_by_fp
            .entry(tag.fingerprint.clone())
            .or_default()
            .push(tag);
    }
    let cover_fps = dao::select_fingerprints_with_covers(&mut conn).await?;
    let files = db_files
        .into_iter()
        .map(|file| {
            let tags = tags_by_fp.remove(&file.fingerprint).unwrap_or_default();
            let has_cover = cover_fps.contains(&file.fingerprint);
            let mut api_file: File = (file, tags).into();
            api_file.has_cover = has_cover;
            api_file
        })
        .collect();
    Ok(Json(files))
}

async fn update_file(
    State(application_module): State<AppState>,
    _user: AuthorizedUser,
    Json(file): Json<File>,
) -> Result<Json<File>> {
    application_module
        .db_client()
        .await
        .update_file(file.clone())
        .await?;
    Ok(Json(file))
}

/// @feature: tags.list
async fn get_files_tags(
    State(application_module): State<AppState>,
    user: AuthorizedUser,
    private_mode: PrivateModeHeader,
) -> Result<Json<Vec<String>>> {
    if private_mode.0 {
        if !user.has_role("owner") {
            return Err(Error::Forbidden(
                "private mode access requires owner role".into(),
            ));
        }
        let pool = application_module.connection_pool().await;
        let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
        let tags = dao::select_all_distinct_tags(&mut conn).await?;
        return Ok(Json(tags));
    }
    let settings = application_module.settings().await;
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let excluded = settings.ui.private_tags().to_vec();
    let tags = dao::select_all_distinct_tags_excluding(&mut conn, &excluded).await?;
    Ok(Json(tags))
}

async fn get_file(
    AxumPath(guid): AxumPath<String>,
    State(application_module): State<AppState>,
    user: AuthorizedUser,
    private_mode: PrivateModeHeader,
) -> Result<Response> {
    let guid = guid.as_str();
    let settings = application_module.settings().await;
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let Some(file) = dao::select_file_by_guid(&mut conn, guid).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let tags = dao::select_content_tags_by_fingerprint(&mut conn, &file.fingerprint).await?;
    if private_mode.0 {
        if !user.has_role("owner") {
            return Err(Error::Forbidden(
                "private mode access requires owner role".into(),
            ));
        }
    } else if settings
        .ui
        .contains_hidden_tag(&tags.iter().map(|t| t.tag.clone()).collect::<Vec<_>>())
    {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    let has_cover = dao::cover_exists(&mut conn, &file.fingerprint).await?;
    let mut api_file: File = (file, tags).into();
    api_file.has_cover = has_cover;
    Ok(Json(api_file).into_response())
}

async fn get_file_tags(
    AxumPath(guid): AxumPath<String>,
    State(application_module): State<AppState>,
    user: AuthorizedUser,
    private_mode: PrivateModeHeader,
) -> Result<Json<Vec<String>>> {
    let guid = guid.as_str();
    let settings = application_module.settings().await;
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let Some(file) = dao::select_file_by_guid(&mut conn, guid).await? else {
        return Ok(Json(vec![]));
    };
    let content_tags =
        dao::select_content_tags_by_fingerprint(&mut conn, &file.fingerprint).await?;
    let tag_strings: Vec<String> = content_tags.iter().map(|t| t.tag.clone()).collect();
    if private_mode.0 {
        if !user.has_role("owner") {
            return Err(Error::Forbidden(
                "private mode access requires owner role".into(),
            ));
        }
    } else if settings.ui.contains_hidden_tag(&tag_strings) {
        return Ok(Json(vec![]));
    }
    Ok(Json(tag_strings))
}

/// @feature: tags.add
async fn post_file_tags(
    AxumPath(guid): AxumPath<String>,
    State(application_module): State<AppState>,
    user: AuthorizedUser,
    private_mode: PrivateModeHeader,
    Json(tags): Json<Vec<String>>,
) -> Result<Json<Vec<String>>> {
    let guid = guid.as_str();
    let settings = application_module.settings().await;
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let Some(file) = dao::select_file_by_guid(&mut conn, guid).await? else {
        return Ok(Json(vec![]));
    };
    let existing_tags = dao::select_content_tags_by_fingerprint(&mut conn, &file.fingerprint)
        .await?
        .iter()
        .map(|t| t.tag.clone())
        .collect::<Vec<_>>();
    if private_mode.0 {
        if !user.has_role("owner") {
            return Err(Error::Forbidden(
                "private mode access requires owner role".into(),
            ));
        }
    } else if settings.ui.contains_hidden_tag(&existing_tags) {
        return Ok(Json(vec![]));
    }
    let content_tags = tags
        .into_iter()
        .map(|tag| crate::db::models::ContentTag::new(file.fingerprint.clone(), tag))
        .collect();
    dao::upsert_many_content_tags(&mut conn, content_tags).await?;
    get_file_tags(
        AxumPath(guid.to_string()),
        State(application_module),
        user,
        private_mode,
    )
    .await
}

/// @feature: tags.remove
async fn delete_file_tags(
    AxumPath(guid): AxumPath<String>,
    State(application_module): State<AppState>,
    user: AuthorizedUser,
    private_mode: PrivateModeHeader,
    Json(tags): Json<Vec<String>>,
) -> Result<Json<Vec<String>>> {
    let guid = guid.as_str();
    let settings = application_module.settings().await;
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let Some(file) = dao::select_file_by_guid(&mut conn, guid).await? else {
        return Ok(Json(vec![]));
    };
    let existing_tags = dao::select_content_tags_by_fingerprint(&mut conn, &file.fingerprint)
        .await?
        .iter()
        .map(|t| t.tag.clone())
        .collect::<Vec<_>>();
    if private_mode.0 {
        if !user.has_role("owner") {
            return Err(Error::Forbidden(
                "private mode access requires owner role".into(),
            ));
        }
    } else if settings.ui.contains_hidden_tag(&existing_tags) {
        return Ok(Json(vec![]));
    }
    dao::delete_content_tags(&mut conn, &file.fingerprint, tags).await?;
    get_file_tags(
        AxumPath(guid.to_string()),
        State(application_module),
        user,
        private_mode,
    )
    .await
}

async fn download_file(
    AxumPath((guid, file_name)): AxumPath<(String, String)>,
    State(application_module): State<AppState>,
    user: AuthorizedUser,
    private_mode: PrivateModeHeader,
) -> Result<Response> {
    let settings = application_module.settings().await;
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let file = dao::select_file_by_guid(&mut conn, &guid).await?;

    let Some(file) = file else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };

    let tags = dao::select_content_tags_by_fingerprint(&mut conn, &file.fingerprint)
        .await?
        .iter()
        .map(|t| t.tag.clone())
        .collect::<Vec<_>>();
    if private_mode.0 {
        if !user.has_role("owner") {
            return Err(Error::Forbidden(
                "private mode access requires owner role".into(),
            ));
        }
    } else if settings.ui.contains_hidden_tag(&tags) {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    if !file_name.ends_with(&file.type_.to_lowercase()) {
        tracing::error!(
            "Incorrect file extension on `{file_name}`, expected `{}`",
            file.type_
        );
        return Ok(StatusCode::NOT_FOUND.into_response());
    }

    let path = Path::new(&file.path);
    if !path.exists() {
        tracing::error!("Database out of sync, file not found: {path:?}");
        return Ok(StatusCode::NOT_FOUND.into_response());
    }

    let content_type = content_type_for_extension(&file.type_)?;
    let data = tokio::fs::read(path).await?;
    Ok(([(header::CONTENT_TYPE, content_type)], data).into_response())
}

async fn get_file_cover(
    AxumPath(guid): AxumPath<String>,
    State(application_module): State<AppState>,
    user: AuthorizedUser,
    private_mode: PrivateModeHeader,
) -> Result<Response> {
    let settings = application_module.settings().await;
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let Some(file) = dao::select_file_by_guid(&mut conn, &guid).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    if private_mode.0 {
        if !user.has_role("owner") {
            return Err(Error::Forbidden(
                "private mode access requires owner role".into(),
            ));
        }
    } else {
        let tags = dao::select_content_tags_by_fingerprint(&mut conn, &file.fingerprint)
            .await?
            .iter()
            .map(|t| t.tag.clone())
            .collect::<Vec<_>>();
        if settings.ui.contains_hidden_tag(&tags) {
            return Ok(StatusCode::NOT_FOUND.into_response());
        }
    }
    let Some((data, mime)) = dao::get_cover(&mut conn, &file.fingerprint).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    Ok(cover_response(data, mime))
}

/// @feature: sources.delete
async fn delete_file(
    AxumPath(guid): AxumPath<String>,
    State(application_module): State<AppState>,
    user: AuthorizedUser,
    private_mode: PrivateModeHeader,
) -> Result<()> {
    let guid = guid.as_str();
    let settings = application_module.settings().await;
    let db_client = application_module.db_client().await;
    let file = db_client.get_file(guid).await?;

    if let Some(ref file) = file {
        if private_mode.0 {
            if !user.has_role("owner") {
                return Err(Error::Forbidden(
                    "private mode access requires owner role".into(),
                ));
            }
        } else if settings.ui.contains_hidden_tag(&file.tags) {
            return Err(Error::FileNotFound(guid.to_string()));
        }
        db_client.delete_file(file.clone()).await?;
        Ok(())
    } else {
        Err(Error::FileNotFound(guid.to_string()))
    }
}

/// @feature: sources.send_to_client
async fn upload_file(
    State(application_module): State<AppState>,
    _user: AuthorizedUser,
    mut multipart: Multipart,
) -> Result<Json<File>> {
    let field = multipart
        .next_field()
        .await
        .map_err(|e| Error::BadRequest(e.to_string()))?
        .ok_or_else(|| Error::BadRequest("no file field in multipart form".into()))?;

    // Read metadata before consuming the field body with `bytes()`.
    let content_type = field.content_type().map(|s| s.to_string());
    let raw_name = field.file_name().map(|s| s.to_string());
    let data = field
        .bytes()
        .await
        .map_err(|e| Error::BadRequest(e.to_string()))?;

    let extension = content_type
        .as_deref()
        .map(content_type_to_extension)
        .transpose()?
        .ok_or_else(|| Error::UnsupportedContentType("missing content-type".into()))?;

    if !matches!(
        extension.to_lowercase().as_str(),
        "pdf" | "epub" | "mobi" | "azw"
    ) {
        return Err(Error::UnsupportedExtension(extension));
    }

    // The sanitized base name (without extension), mirroring the previous
    // `TempFile::name()` behaviour.
    let filename = raw_name
        .as_deref()
        .and_then(|n| Path::new(n).file_stem())
        .and_then(|s| s.to_str())
        .ok_or_else(|| Error::BadRequest("missing file name".into()))?;

    let target_dir = application_module
        .settings()
        .await
        .server
        .download_folder
        .join(filename);

    if !target_dir.exists() {
        tokio::fs::create_dir(&target_dir).await?;
    }

    let mut target_file = target_dir.join(format!("{filename}.{extension}"));

    to_unique_file(&mut target_file, &extension);

    tokio::fs::write(&target_file, &data).await?;

    application_module.scan(target_file.clone()).await?;

    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let result = dao::select_file_by_path(&mut conn, &canonical_path_string(&target_file))
        .await?
        .ok_or_else(|| {
            Error::Scan("file not recorded after scan; server may be in dry-run mode".to_string())
        })?;
    Ok(Json((result, vec![]).into()))
}

async fn get_reading_state(
    AxumPath(fingerprint): AxumPath<String>,
    State(application_module): State<AppState>,
    _user: AuthorizedUser,
) -> Result<Response> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let state = dao::get_reading_state(&mut conn, &fingerprint).await?;
    Ok(match state {
        Some(state) => Json(state).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    })
}

/// @feature: reading.progress
async fn put_reading_state(
    State(application_module): State<AppState>,
    _user: AuthorizedUser,
    Json(state): Json<ReadingState>,
) -> Result<Json<ReadingState>> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let result = dao::upsert_reading_state(&mut conn, state).await?;
    Ok(Json(result))
}

#[derive(serde::Deserialize)]
struct ReadingStatusRequest {
    status: ReadingStatus,
}

/// @feature: reading.status
async fn put_reading_status(
    AxumPath(fingerprint): AxumPath<String>,
    State(application_module): State<AppState>,
    _user: AuthorizedUser,
    Json(req): Json<ReadingStatusRequest>,
) -> Result<()> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    dao::update_reading_status_only(&mut conn, &fingerprint, req.status.into()).await?;
    Ok(())
}

// ─── Document routes ──────────────────────────────────────────────────────────

/// @feature: documents.list
async fn get_documents(
    State(application_module): State<AppState>,
    _user: AuthorizedUser,
) -> Result<Json<Vec<ApiDocument>>> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let docs = dao::select_all_api_documents(&mut conn).await?;
    Ok(Json(docs))
}

/// @feature: documents.detail_view
async fn get_document(
    AxumPath(guid): AxumPath<String>,
    State(application_module): State<AppState>,
    _user: AuthorizedUser,
) -> Result<Response> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let doc = dao::select_api_document_by_guid(&mut conn, &guid).await?;
    Ok(match doc {
        Some(doc) => Json(doc).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    })
}

/// @feature: documents.cover_display
async fn get_document_cover(
    AxumPath(guid): AxumPath<String>,
    State(application_module): State<AppState>,
    _user: AuthorizedUser,
) -> Result<Response> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let Some(doc) = dao::select_document_by_guid(&mut conn, &guid).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let Some((data, mime)) = dao::get_document_selected_cover(&mut conn, doc.id).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    Ok(cover_response(data, mime))
}

/// @feature: documents.edit_metadata
/// @feature: documents.select_cover
async fn put_document_metadata(
    AxumPath(guid): AxumPath<String>,
    State(application_module): State<AppState>,
    _user: AuthorizedUser,
    Json(meta): Json<DocumentMeta>,
) -> Result<Json<ApiDocument>> {
    let guid = guid.as_str();
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;

    let doc_row = dao::select_document_by_guid(&mut conn, guid)
        .await?
        .ok_or_else(|| Error::FileNotFound(guid.to_string()))?;

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

    let updated = dao::select_api_document_by_guid(&mut conn, guid)
        .await?
        .expect("document must exist after upsert");
    Ok(Json(updated))
}

async fn ensure_document_for_file(
    AxumPath(guid): AxumPath<String>,
    State(application_module): State<AppState>,
    _user: AuthorizedUser,
) -> Result<Json<ApiDocument>> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let doc = dao::ensure_document_for_file_guid(&mut conn, &guid).await?;
    Ok(Json(doc))
}

/// Admin endpoints require the `owner` role regardless of private mode.
fn require_owner(user: &AuthorizedUser) -> Result<()> {
    if user.has_role("owner") {
        Ok(())
    } else {
        Err(Error::Forbidden("admin actions require owner role".into()))
    }
}

/// @feature: admin.scan
async fn post_scan(
    State(application_module): State<AppState>,
    user: AuthorizedUser,
) -> Result<Json<ScanSummary>> {
    require_owner(&user)?;
    let summary = application_module.scan_configured().await?;
    Ok(Json(summary))
}

#[derive(serde::Serialize)]
struct CheckMissingResponse {
    missing: Vec<String>,
    purged: bool,
}

#[derive(serde::Deserialize)]
struct CheckMissingQuery {
    #[serde(default)]
    purge: Option<bool>,
}

/// @feature: admin.check_missing
async fn post_check_missing(
    State(application_module): State<AppState>,
    user: AuthorizedUser,
    Query(query): Query<CheckMissingQuery>,
) -> Result<Json<CheckMissingResponse>> {
    require_owner(&user)?;
    let purge = query.purge.unwrap_or(false);
    let missing = application_module.check_missing(purge).await;
    Ok(Json(CheckMissingResponse {
        missing,
        purged: purge,
    }))
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ScanDirectoryEntry {
    path: String,
    #[serde(flatten)]
    settings: DirectorySettings,
}

fn list_scan_directories(settings: &Settings) -> Vec<ScanDirectoryEntry> {
    settings
        .scan
        .directories
        .iter()
        .map(|(path, settings)| ScanDirectoryEntry {
            path: path.display().to_string(),
            settings: settings.clone(),
        })
        .collect()
}

/// @feature: admin.scan_directories
async fn get_scan_directories(
    State(application_module): State<AppState>,
    user: AuthorizedUser,
) -> Result<Json<Vec<ScanDirectoryEntry>>> {
    require_owner(&user)?;
    let settings = application_module.settings().await;
    Ok(Json(list_scan_directories(&settings)))
}

/// @feature: admin.scan_directories
async fn put_scan_directory(
    State(application_module): State<AppState>,
    user: AuthorizedUser,
    Json(entry): Json<ScanDirectoryEntry>,
) -> Result<Json<Vec<ScanDirectoryEntry>>> {
    require_owner(&user)?;
    let ScanDirectoryEntry { path, settings } = entry;
    let path = ExpandedPath::from_str(&path)
        .map_err(|e| Error::BadRequest(format!("invalid path: {e}")))?;
    application_module
        .update_settings(Box::new(move |s: &mut Settings| {
            s.scan.directories.insert(path, settings);
        }))
        .await?;
    let settings = application_module.settings().await;
    Ok(Json(list_scan_directories(&settings)))
}

#[derive(serde::Deserialize)]
struct PathQuery {
    path: String,
}

/// @feature: admin.scan_directories
async fn delete_scan_directory(
    State(application_module): State<AppState>,
    user: AuthorizedUser,
    Query(query): Query<PathQuery>,
) -> Result<Json<Vec<ScanDirectoryEntry>>> {
    require_owner(&user)?;
    let parsed = ExpandedPath::from_str(&query.path)
        .map_err(|e| Error::BadRequest(format!("invalid path: {e}")))?;
    application_module
        .update_settings(Box::new(move |s: &mut Settings| {
            s.scan.directories.remove(&parsed);
        }))
        .await?;
    let settings = application_module.settings().await;
    Ok(Json(list_scan_directories(&settings)))
}

/// Editable server settings. `database_url` is informational/read-only — it is
/// returned for display but ignored on PUT (changing the DB at runtime would
/// require rebuilding the connection pool).
#[derive(serde::Serialize, serde::Deserialize)]
struct ServerSettingsDto {
    #[serde(default)]
    database_url: String,
    extensions: Vec<DocumentType>,
    dry_run: bool,
    concurrency: usize,
    private_mode: bool,
    private_tags: Vec<String>,
}

fn server_settings_dto(settings: &Settings) -> ServerSettingsDto {
    ServerSettingsDto {
        database_url: settings.database.url().display().to_string(),
        extensions: settings.scan.extensions.clone(),
        dry_run: settings.scan.dry_run,
        concurrency: settings.scan.concurrency,
        private_mode: settings.ui.private_mode(),
        private_tags: settings.ui.private_tags().to_vec(),
    }
}

/// @feature: admin.server_settings
async fn get_settings(
    State(application_module): State<AppState>,
    user: AuthorizedUser,
) -> Result<Json<ServerSettingsDto>> {
    require_owner(&user)?;
    let settings = application_module.settings().await;
    Ok(Json(server_settings_dto(&settings)))
}

/// @feature: admin.server_settings
async fn put_settings(
    State(application_module): State<AppState>,
    user: AuthorizedUser,
    Json(dto): Json<ServerSettingsDto>,
) -> Result<Json<ServerSettingsDto>> {
    require_owner(&user)?;
    application_module
        .update_settings(Box::new(move |s: &mut Settings| {
            s.scan.extensions = dto.extensions;
            s.scan.dry_run = dto.dry_run;
            s.scan.concurrency = dto.concurrency;
            s.ui.set_private_mode(dto.private_mode);
            s.ui.set_private_tags(dto.private_tags);
        }))
        .await?;
    let settings = application_module.settings().await;
    Ok(Json(server_settings_dto(&settings)))
}

/// A user as exposed over the API. The password hash is NEVER included.
#[derive(serde::Serialize)]
struct UserDto {
    user_id: String,
    roles: Vec<String>,
}

#[derive(serde::Deserialize)]
struct CreateUserRequest {
    user_id: String,
    password: String,
    #[serde(default)]
    roles: Vec<String>,
}

#[derive(serde::Deserialize)]
struct UpdateUserRequest {
    /// When omitted/empty the existing password is kept.
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    roles: Vec<String>,
}

fn make_user_entry(password: HashedPassword, roles: Vec<String>) -> UserEntry {
    if roles.is_empty() {
        UserEntry::Simple(password)
    } else {
        UserEntry::Extended { password, roles }
    }
}

fn list_users(settings: &Settings) -> Vec<UserDto> {
    settings
        .server
        .authorized_users
        .iter()
        .map(|(user_id, entry)| UserDto {
            user_id: user_id.clone(),
            roles: entry.roles().to_vec(),
        })
        .collect()
}

fn hash_password(plain: &str) -> Result<HashedPassword> {
    HashedPassword::try_from(plain.to_string())
        .map_err(|e| Error::Settings(format!("could not hash password: {e}")))
}

/// @feature: admin.authorized_users
async fn get_users(
    State(application_module): State<AppState>,
    user: AuthorizedUser,
) -> Result<Json<Vec<UserDto>>> {
    require_owner(&user)?;
    let settings = application_module.settings().await;
    Ok(Json(list_users(&settings)))
}

/// @feature: admin.authorized_users
async fn post_user(
    State(application_module): State<AppState>,
    user: AuthorizedUser,
    Json(req): Json<CreateUserRequest>,
) -> Result<Json<Vec<UserDto>>> {
    require_owner(&user)?;
    let CreateUserRequest {
        user_id,
        password,
        roles,
    } = req;
    if user_id.is_empty() {
        return Err(Error::BadRequest("user_id must not be empty".into()));
    }
    if application_module
        .settings()
        .await
        .server
        .authorized_users
        .contains_key(&user_id)
    {
        return Err(Error::BadRequest(format!("user {user_id} already exists")));
    }
    let entry = make_user_entry(hash_password(&password)?, roles);
    application_module
        .update_settings(Box::new(move |s: &mut Settings| {
            s.server.authorized_users.insert(user_id, entry);
        }))
        .await?;
    let settings = application_module.settings().await;
    Ok(Json(list_users(&settings)))
}

/// @feature: admin.authorized_users
async fn put_user(
    AxumPath(user_id): AxumPath<String>,
    State(application_module): State<AppState>,
    user: AuthorizedUser,
    Json(req): Json<UpdateUserRequest>,
) -> Result<Json<Vec<UserDto>>> {
    require_owner(&user)?;
    let user_id = user_id.as_str();
    let UpdateUserRequest { password, roles } = req;

    let existing = application_module
        .settings()
        .await
        .server
        .authorized_users
        .get(user_id)
        .cloned();
    let Some(existing) = existing else {
        return Err(Error::FileNotFound(format!("user {user_id}")));
    };

    let password_hash = match password {
        Some(p) if !p.is_empty() => hash_password(&p)?,
        _ => existing.password().clone(),
    };
    let entry = make_user_entry(password_hash, roles);
    let id = user_id.to_string();
    application_module
        .update_settings(Box::new(move |s: &mut Settings| {
            s.server.authorized_users.insert(id, entry);
        }))
        .await?;
    let settings = application_module.settings().await;
    Ok(Json(list_users(&settings)))
}

/// @feature: admin.authorized_users
async fn delete_user(
    AxumPath(user_id): AxumPath<String>,
    State(application_module): State<AppState>,
    user: AuthorizedUser,
) -> Result<Json<Vec<UserDto>>> {
    require_owner(&user)?;
    let user_id = user_id.as_str();
    if user_id == user.user_id {
        return Err(Error::BadRequest(
            "you cannot delete the user you are authenticated as".into(),
        ));
    }
    let id = user_id.to_string();
    application_module
        .update_settings(Box::new(move |s: &mut Settings| {
            s.server.authorized_users.shift_remove(&id);
        }))
        .await?;
    let settings = application_module.settings().await;
    Ok(Json(list_users(&settings)))
}

/// @feature: documents.merge
async fn post_merge_documents(
    State(application_module): State<AppState>,
    _user: AuthorizedUser,
    Json(req): Json<MergeDocumentsRequest>,
) -> Result<Json<ApiDocument>> {
    let pool = application_module.connection_pool().await;
    dao::merge_documents(&pool, &req.winner_guid, &req.loser_guids).await?;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let doc = dao::select_api_document_by_guid(&mut conn, &req.winner_guid)
        .await?
        .ok_or_else(|| Error::FileNotFound(req.winner_guid.clone()))?;
    Ok(Json(doc))
}

#[derive(serde::Serialize)]
struct OnlineLibrarySearchResponse {
    books: Vec<OnlineBook>,
    catalogs: Vec<OnlineCatalog>,
}

#[derive(serde::Deserialize)]
struct SearchQuery {
    q: String,
}

/// @feature: online_library.search
async fn search_online_library(
    State(application_module): State<AppState>,
    _user: AuthorizedUser,
    Query(SearchQuery { q }): Query<SearchQuery>,
) -> Result<Json<OnlineLibrarySearchResponse>> {
    let settings = application_module.settings().await;
    let catalogs: Vec<OnlineCatalog> = settings
        .online_library
        .catalogs
        .iter()
        .filter(|catalog| catalog.enabled)
        .cloned()
        .collect();

    let searches = catalogs.iter().cloned().map(|catalog| {
        let q = q.clone();
        async move {
            let catalog_name = catalog.name.clone();
            let client = OpdsClient::new(catalog);
            match client.search_with_next(&q).await {
                Ok((books, _next_url)) => books,
                Err(e) => {
                    tracing::warn!("OPDS search of {catalog_name} failed: {e}");
                    vec![]
                }
            }
        }
    });
    let books = futures::future::join_all(searches).await.concat();

    Ok(Json(OnlineLibrarySearchResponse { books, catalogs }))
}

#[derive(serde::Deserialize)]
struct ImportOnlineBookRequest {
    title: String,
    format: DownloadFormat,
}

/// @feature: online_library.download_import
async fn import_online_book(
    State(application_module): State<AppState>,
    _user: AuthorizedUser,
    Json(req): Json<ImportOnlineBookRequest>,
) -> Result<Json<File>> {
    let download_folder = application_module.settings().await.server.download_folder;
    let path = download_book(&req.format, &req.title, &download_folder)
        .await
        .map_err(|e| Error::Scan(e.to_string()))?;
    application_module.scan(path.clone()).await?;
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let result = dao::select_file_by_path(&mut conn, &canonical_path_string(&path))
        .await?
        .ok_or_else(|| Error::FileNotFound(path.display().to_string()))?;
    Ok(Json((result, vec![]).into()))
}

/// Build an image response for a cover, using the stored MIME type and falling
/// back to `image/jpeg` when it is missing or not a valid header value.
fn cover_response(data: Vec<u8>, mime: String) -> Response {
    let mime = if mime.trim().is_empty() {
        "image/jpeg".to_string()
    } else {
        mime
    };
    match axum::http::HeaderValue::from_str(&mime) {
        Ok(value) => ([(header::CONTENT_TYPE, value)], data).into_response(),
        Err(_) => (
            [(
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("image/jpeg"),
            )],
            data,
        )
            .into_response(),
    }
}

/// Path string used to look a file up after a scan. `scan` canonicalizes the
/// path before storing it (`ApplicationModule::start_scan`), so the lookup must
/// canonicalize too — otherwise a symlinked download folder (e.g. macOS
/// `/var` → `/private/var`) yields a mismatch and the freshly-scanned file
/// isn't found. Falls back to the raw path if canonicalization fails.
fn canonical_path_string(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

/// Map a file extension to its MIME type for downloads.
fn content_type_for_extension(extension: &str) -> Result<&'static str> {
    Ok(match extension.to_lowercase().as_str() {
        "pdf" => "application/pdf",
        "epub" => "application/epub+zip",
        "mobi" | "prc" | "azw" => "application/x-mobipocket-ebook",
        "fb2" => "application/x-fictionbook+xml",
        "cbz" => "application/vnd.comicbook+zip",
        "cbt" => "application/vnd.comicbook+tar",
        _ => return Err(Error::UnsupportedExtension(extension.to_string())),
    })
}

/// Map an uploaded content-type (MIME, possibly with parameters) to a file
/// extension.
fn content_type_to_extension(content_type: &str) -> Result<String> {
    let base = content_type
        .split(';')
        .next()
        .unwrap_or(content_type)
        .trim()
        .to_lowercase();
    Ok(match base.as_str() {
        "application/pdf" => "pdf",
        "application/epub+zip" => "epub",
        "application/x-mobipocket-ebook" => "mobi",
        "application/x-fictionbook+xml" => "fb2",
        "application/vnd.comicbook+zip" => "cbz",
        "application/vnd.comicbook+tar" => "cbt",
        _ => return Err(Error::UnsupportedContentType(content_type.to_string())),
    }
    .to_string())
}
