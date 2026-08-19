// SPDX-License-Identifier: AGPL-3.0-or-later

mod access;
mod authn;
mod token;

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use access::Visibility;
use authn::AuthorizedUser;
use axum::Json;
use axum::Router;
use axum::extract::Form;
use axum::extract::Multipart;
use axum::extract::Path as AxumPath;
use axum::extract::Query;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::http::HeaderName;
use axum::http::HeaderValue;
use axum::http::StatusCode;
use axum::http::header;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::routing::get;
use axum::routing::post;
use axum::routing::put;
use axum_server::Handle;
pub use axum_server::tls_rustls::RustlsConfig;
use figment::Figment;
use provider::r#async::AndThen;
use provider::r#async::Provider;
use token::TokenService;
use tokio::net::TcpListener;
use tower_governor::GovernorLayer;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::GlobalKeyExtractor;
use tower_http::cors::Any;
use tower_http::cors::CorsLayer;

#[cfg(feature = "embed-pwa")]
mod spa;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::DefaultMakeSpan;
use tower_http::trace::DefaultOnRequest;
use tower_http::trace::DefaultOnResponse;
use tower_http::trace::TraceLayer;
use tracing::Level;

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
use crate::online_library::Catalog;
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
/// as before. Also carries the [`TokenService`] used to issue/verify Bearer
/// tokens.
#[derive(Clone)]
pub struct AppState {
    module: Arc<dyn ServerModule>,
    tokens: Arc<TokenService>,
    basic_limiter: Arc<authn::BasicAuthLimiter>,
}

impl AppState {
    pub fn new(module: Arc<dyn ServerModule>) -> Self {
        Self {
            module,
            tokens: Arc::new(TokenService::generate()),
            basic_limiter: Arc::new(authn::BasicAuthLimiter::default()),
        }
    }

    /// The token issuer/verifier for this server instance.
    pub(crate) fn tokens(&self) -> &TokenService {
        &self.tokens
    }

    /// Failure-budget limiter shared by all Basic-auth verification paths.
    pub(crate) fn basic_limiter(&self) -> &authn::BasicAuthLimiter {
        &self.basic_limiter
    }
}

impl std::ops::Deref for AppState {
    type Target = dyn ServerModule;

    fn deref(&self) -> &Self::Target {
        &*self.module
    }
}

/// CORS policy: any method, an explicit header allow-list, origin restricted to
/// `[server].allowed_origins` when set. Empty list = any origin (with a warning).
///
/// Headers are listed explicitly rather than `*` because the CORS wildcard does
/// **not** cover `Authorization` (per the Fetch spec), which every request uses.
fn cors_layer(allowed_origins: &[String]) -> CorsLayer {
    let headers = [
        header::AUTHORIZATION,
        header::CONTENT_TYPE,
        header::ACCEPT,
        HeaderName::from_static("x-private-mode"),
    ];
    let base = CorsLayer::new().allow_methods(Any).allow_headers(headers);
    if allowed_origins.is_empty() {
        tracing::warn!(
            "CORS is unrestricted (any origin allowed); set [server].allowed_origins to restrict it"
        );
        base.allow_origin(Any)
    } else {
        let origins: Vec<HeaderValue> = allowed_origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect();
        base.allow_origin(origins)
    }
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

/// Build the fully-configured router (routes + security layers + state).
/// Reads `[server]` for CORS, upload limit, and TLS/HSTS. Exposed so the COSMIC
/// app can embed the server in-process and serve it on its own runtime.
pub async fn build_router(state: AppState) -> Router {
    let server = state.settings().await.server;
    let max_upload = server
        .max_upload_bytes
        .unwrap_or(settings::DEFAULT_MAX_UPLOAD_BYTES) as usize;

    // Rate-limit the token endpoint to blunt password brute-forcing. Global key
    // (per-IP needs ConnectInfo; a reverse proxy is the recommended per-IP path).
    let governor = GovernorLayer::new(std::sync::Arc::new(
        GovernorConfigBuilder::default()
            .per_second(2)
            .burst_size(10)
            .key_extractor(GlobalKeyExtractor)
            .finish()
            .expect("governor config"),
    ));

    let routes = Router::new()
        .route("/status", get(status))
        .route("/ca.pem", get(get_ca_cert))
        .route(
            "/files",
            get(get_files)
                .put(update_file)
                .post(upload_file)
                .layer(RequestBodyLimitLayer::new(max_upload)),
        )
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
        .route("/files/{guid}/pdf/page-count", get(get_pdf_page_count))
        .route(
            "/files/{guid}/pdf/page/{index}/preview",
            get(get_pdf_page_preview),
        )
        .route(
            "/files/{guid}/pdf/page/{index}/thumbnail",
            post(post_pdf_page_thumbnail),
        )
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
        .route("/oauth/token", post(oauth_token).layer(governor));

    // Serve the embedded PWA for any unmatched route so the security/trace
    // layers below also wrap the static responses (feature `embed-pwa`).
    #[cfg(feature = "embed-pwa")]
    let routes = routes.fallback(spa::handler);

    let mut router = routes
        .layer(cors_layer(&server.allowed_origins))
        // Baseline security headers (HSTS added below only when TLS is on).
        .layer(SetResponseHeaderLayer::if_not_present(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::if_not_present(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
        // Outermost: log every request/response (method, path, status, latency)
        // at INFO. The per-handler `#[instrument]` spans nest under this.
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_request(DefaultOnRequest::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        );

    // HSTS only makes sense over HTTPS.
    if server.tls.is_some() {
        router = router.layer(SetResponseHeaderLayer::if_not_present(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=31536000"),
        ));
    }

    router.with_state(state)
}

/// Build the router directly from a configuration file. Convenience entry point
/// for embedding the server.
pub async fn build_app(config_path: PathBuf) -> anyhow::Result<Router> {
    Ok(build_router(build_state(config_path).await?).await)
}

/// Filename the local CA's certificate is always written/looked up under,
/// within whatever directory generation was pointed at (the config file's
/// parent directory in practice). Fixed and predictable so callers (the
/// COSMIC preferences handler, the `/ca.pem` route) don't need to persist
/// the path anywhere themselves — see [`ca_cert_path`].
const CA_CERT_FILENAME: &str = "read-flow-ca-cert.pem";
/// Counterpart to [`CA_CERT_FILENAME`] for the CA's private key.
const CA_KEY_FILENAME: &str = "read-flow-ca-key.pem";

/// The local CA cert path within `dir`, if [`generate_local_ca`] has been
/// run there (callers should check [`Path::exists`]).
pub fn ca_cert_path(dir: &Path) -> PathBuf {
    dir.join(CA_CERT_FILENAME)
}

/// The local CA key path within `dir`, counterpart to [`ca_cert_path`].
pub fn ca_key_path(dir: &Path) -> PathBuf {
    dir.join(CA_KEY_FILENAME)
}

/// Best-effort local (LAN) IP address of this machine, for including in a
/// generated cert's SANs when the server is bound to `0.0.0.0` (reachable
/// from the LAN, not just loopback). Uses the "connect a UDP socket, read
/// back the local address the OS picked" trick — no packets are actually
/// sent (UDP `connect` only sets up local routing), so this works offline
/// and doesn't depend on any extra crate. `None` if there's no route at all
/// (e.g. no network interface up).
pub fn detect_lan_ip() -> Option<std::net::IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|addr| addr.ip())
}

/// Generate an ECDSA key pair and a self-signed certificate covering `sans`
/// (DNS names; defaults to `localhost`), writing `read-flow-cert.pem` and
/// `read-flow-key.pem` into `dir`. Returns `(cert_path, key_path)`.
///
/// Self-signed certs work for our own COSMIC client but browsers/PWAs will not
/// trust them without manually importing the certificate.
pub fn generate_self_signed_cert(
    dir: &Path,
    sans: Vec<String>,
) -> anyhow::Result<(PathBuf, PathBuf)> {
    let sans = if sans.is_empty() {
        vec!["localhost".to_string()]
    } else {
        sans
    };
    let generated = rcgen::generate_simple_self_signed(sans)?;
    let cert_path = dir.join("read-flow-cert.pem");
    let key_path = dir.join("read-flow-key.pem");
    std::fs::write(&cert_path, generated.cert.pem())?;
    std::fs::write(&key_path, generated.signing_key.serialize_pem())?;
    Ok((cert_path, key_path))
}

/// Generate a long-lived (10 year) local root CA, writing
/// `read-flow-ca-cert.pem` and `read-flow-ca-key.pem` into `dir`. Returns
/// `(ca_cert_path, ca_key_path)`.
///
/// This is the "trust me once" root: a client trusts this single cert on
/// each device (e.g. by importing the root into its OS/browser trust
/// store), and every leaf cert issued afterwards via
/// [`generate_ca_signed_cert`] against the *same* CA files is automatically
/// trusted too — regenerating a leaf (e.g. after the bind address changes)
/// never requires re-trusting anything.
pub fn generate_local_ca(dir: &Path) -> anyhow::Result<(PathBuf, PathBuf)> {
    let mut params = rcgen::CertificateParams::new(Vec::new())?;
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "Read Flow Local CA");
    params
        .key_usages
        .push(rcgen::KeyUsagePurpose::DigitalSignature);
    params.key_usages.push(rcgen::KeyUsagePurpose::KeyCertSign);
    params.key_usages.push(rcgen::KeyUsagePurpose::CrlSign);
    let now = time::OffsetDateTime::now_utc();
    // Back-dated slightly to tolerate clock skew between devices, matching
    // the pattern rcgen's own examples use.
    params.not_before = now - time::Duration::days(1);
    params.not_after = now + time::Duration::days(3653); // ~10 years

    let key_pair = rcgen::KeyPair::generate()?;
    let cert = params.self_signed(&key_pair)?;

    let cert_path = ca_cert_path(dir);
    let key_path = ca_key_path(dir);
    std::fs::write(&cert_path, cert.pem())?;
    std::fs::write(&key_path, key_pair.serialize_pem())?;
    Ok((cert_path, key_path))
}

/// Issue a leaf cert for `sans` using the local CA in `dir`, reusing it if
/// [`generate_local_ca`] already ran there and generating one fresh
/// otherwise. This is the entry point COSMIC's "Generate certificate"
/// preferences action should use: regenerating a leaf (e.g. after the bind
/// address changes) reuses the same CA, so devices that already trusted it
/// never need to re-trust anything.
pub fn generate_or_reuse_ca_signed_cert(
    dir: &Path,
    sans: Vec<String>,
) -> anyhow::Result<(PathBuf, PathBuf)> {
    let cert_path = ca_cert_path(dir);
    let key_path = ca_key_path(dir);
    if !cert_path.exists() || !key_path.exists() {
        generate_local_ca(dir)?;
    }
    generate_ca_signed_cert(dir, &cert_path, &key_path, sans)
}

/// Issue a leaf certificate covering `sans` (DNS names/IPs; defaults to
/// `localhost`), signed by the CA at `ca_cert_path`/`ca_key_path` (as
/// generated by [`generate_local_ca`]). Writes `read-flow-cert.pem` and
/// `read-flow-key.pem` into `dir` — the same filenames
/// [`generate_self_signed_cert`] uses, so [`crate::settings::TlsSettings`]
/// doesn't need a different shape for CA-issued vs. self-signed certs.
///
/// Unlike a self-signed cert, this one is trusted by any client that trusts
/// the CA root — no per-cert trust step.
pub fn generate_ca_signed_cert(
    dir: &Path,
    ca_cert_path: &Path,
    ca_key_path: &Path,
    sans: Vec<String>,
) -> anyhow::Result<(PathBuf, PathBuf)> {
    let sans = if sans.is_empty() {
        vec!["localhost".to_string()]
    } else {
        sans
    };

    let ca_cert_pem = std::fs::read_to_string(ca_cert_path)?;
    let ca_key_pem = std::fs::read_to_string(ca_key_path)?;
    let ca_key = rcgen::KeyPair::from_pem(&ca_key_pem)?;
    let issuer = rcgen::Issuer::from_ca_cert_pem(&ca_cert_pem, ca_key)?;

    let mut params = rcgen::CertificateParams::new(sans)?;
    let now = time::OffsetDateTime::now_utc();
    params.not_before = now - time::Duration::days(1);
    // ~27 months: under the CA/Browser Forum's max leaf lifetime, well past
    // any realistic time between regenerations.
    params.not_after = now + time::Duration::days(825);
    params.use_authority_key_identifier_extension = true;
    params
        .key_usages
        .push(rcgen::KeyUsagePurpose::DigitalSignature);
    params
        .extended_key_usages
        .push(rcgen::ExtendedKeyUsagePurpose::ServerAuth);

    let leaf_key = rcgen::KeyPair::generate()?;
    let leaf_cert = params.signed_by(&leaf_key, &issuer)?;

    let cert_path = dir.join("read-flow-cert.pem");
    let key_path = dir.join("read-flow-key.pem");
    std::fs::write(&cert_path, leaf_cert.pem())?;
    std::fs::write(&key_path, leaf_key.serialize_pem())?;
    Ok((cert_path, key_path))
}

/// Load a rustls config from the configured cert/key PEM files, if TLS is set.
pub async fn load_tls(
    tls: &Option<crate::settings::TlsSettings>,
) -> anyhow::Result<Option<RustlsConfig>> {
    match tls {
        None => Ok(None),
        Some(tls) => {
            let config = RustlsConfig::from_pem_file(tls.cert.as_path(), tls.key.as_path()).await?;
            Ok(Some(config))
        }
    }
}

/// Serve an already-built router on the given listener until shutdown. Speaks
/// HTTPS when `tls` is provided, plain HTTP otherwise.
pub async fn serve_on(
    listener: TcpListener,
    app: Router,
    tls: Option<RustlsConfig>,
) -> std::io::Result<()> {
    match tls {
        None => axum::serve(listener, app).await,
        Some(config) => {
            let std_listener = listener.into_std()?;
            axum_server::from_tcp_rustls(std_listener, config)?
                .serve(app.into_make_service())
                .await
        }
    }
}

/// Serve until either the process ends or `shutdown` resolves. The shutdown
/// hook is how the embedding app (COSMIC) stops/restarts the server: complete
/// the future and the server drains in-flight requests and returns. Speaks
/// HTTPS when `tls` is provided.
pub async fn serve_on_with_shutdown(
    listener: TcpListener,
    app: Router,
    tls: Option<RustlsConfig>,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    match tls {
        None => {
            axum::serve(listener, app)
                .with_graceful_shutdown(shutdown)
                .await
        }
        Some(config) => {
            let handle = Handle::new();
            tokio::spawn({
                let handle = handle.clone();
                async move {
                    shutdown.await;
                    handle.graceful_shutdown(Some(std::time::Duration::from_secs(5)));
                }
            });
            let std_listener = listener.into_std()?;
            axum_server::from_tcp_rustls(std_listener, config)?
                .handle(handle)
                .serve(app.into_make_service())
                .await
        }
    }
}

pub async fn main(config_path: PathBuf) -> anyhow::Result<()> {
    let state = build_state(config_path).await?;
    let server = state.settings().await.server;
    let addr = server.bind_addr();
    let tls = load_tls(&server.tls).await?;
    let listener = TcpListener::bind(addr).await?;
    // Printed to stdout (tracing goes to stderr) so test/e2e harnesses can
    // parse the bound address, which matters when `port = 0`.
    let scheme = if tls.is_some() { "https" } else { "http" };
    println!("Server listening on {scheme}://{}", listener.local_addr()?);
    let app = build_router(state).await;
    serve_on(listener, app, tls).await?;
    Ok(())
}

// ─── OAuth2 token endpoint ────────────────────────────────────────────────────

/// RFC 6749 §4.3 token request (form-encoded). The resource-owner credentials
/// may also arrive via the `Authorization: Basic` header, which takes priority.
#[derive(serde::Deserialize)]
struct TokenRequest {
    grant_type: Option<String>,
    username: Option<String>,
    password: Option<String>,
}

/// RFC 6749 §5.1 successful token response.
#[derive(serde::Serialize)]
struct TokenResponse {
    access_token: String,
    token_type: &'static str,
    expires_in: u64,
    scope: String,
}

/// RFC 6749 §5.2 error response.
#[derive(serde::Serialize)]
struct OAuthErrorBody {
    error: &'static str,
    error_description: String,
}

fn oauth_error(
    status: StatusCode,
    error: &'static str,
    description: impl Into<String>,
) -> Response {
    (
        status,
        Json(OAuthErrorBody {
            error,
            error_description: description.into(),
        }),
    )
        .into_response()
}

/// Resource-owner credentials: prefer the `Authorization: Basic` header, else
/// the form body's `username`/`password`.
fn resource_owner_credentials(headers: &HeaderMap, req: &TokenRequest) -> Option<(String, String)> {
    if let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        && value.to_lowercase().starts_with("basic ")
        && let Ok(pair) = AuthorizedUser::extract_basic_auth(value)
    {
        return Some(pair);
    }
    match (req.username.clone(), req.password.clone()) {
        (Some(username), Some(password)) => Some((username, password)),
        _ => None,
    }
}

/// @feature: admin.local_ca
/// `GET /ca.pem` — the local CA root certificate, if TLS is configured with
/// one generated via [`generate_local_ca`]/[`generate_or_reuse_ca_signed_cert`].
/// Deliberately unauthenticated: this is the bootstrap artifact a device
/// needs to trust *before* it can make a trusted HTTPS connection at all
/// (and often before it even has credentials), and a CA certificate — unlike
/// its private key, which this never serves — isn't sensitive; every device
/// gets the same public trust anchor.
#[tracing::instrument(skip_all)]
async fn get_ca_cert(State(state): State<AppState>) -> Response {
    let settings = state.settings().await;
    let Some(tls) = settings.server.tls else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let dir = tls
        .cert
        .as_path()
        .parent()
        .unwrap_or_else(|| Path::new("."));
    match tokio::fs::read_to_string(ca_cert_path(dir)).await {
        Ok(pem) => ([(header::CONTENT_TYPE, "application/x-pem-file")], pem).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// `POST /oauth/token` — exchange Basic (or form) credentials for a Bearer JWT.
/// This is the one place that still runs PBKDF2; every other endpoint can then
/// present the fast-to-verify token.
#[tracing::instrument(skip_all)]
async fn oauth_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(req): Form<TokenRequest>,
) -> Response {
    if let Some(grant_type) = &req.grant_type
        && grant_type != "password"
    {
        return oauth_error(
            StatusCode::BAD_REQUEST,
            "unsupported_grant_type",
            format!("grant_type '{grant_type}' is not supported"),
        );
    }

    let Some((user_id, password)) = resource_owner_credentials(&headers, &req) else {
        return oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "missing resource owner credentials",
        );
    };

    let settings = state.settings().await;
    // Verify on the blocking pool: Argon2/PBKDF2 must not stall async workers.
    let entry = settings.server.authorized_users.get(&user_id).cloned();
    let verified = tokio::task::spawn_blocking(move || match entry {
        Some(entry) => entry
            .password()
            .verify(&password)
            .is_ok()
            .then(|| entry.roles().to_vec()),
        None => {
            // Match the timing of a real verify (anti username-enumeration).
            settings::verify_dummy(&password);
            None
        }
    })
    .await
    .ok()
    .flatten();
    let Some(roles) = verified else {
        return oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_grant",
            "invalid username or password",
        );
    };

    match state.tokens().issue(&user_id, &roles) {
        Ok(access_token) => {
            let body = TokenResponse {
                access_token,
                token_type: "Bearer",
                expires_in: state.tokens().ttl_seconds(),
                scope: roles.join(" "),
            };
            let mut response = Json(body).into_response();
            // RFC 6749 §5.1: tokens must not be cached.
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            response
        }
        Err(error) => {
            tracing::error!("could not issue token: {error}");
            oauth_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "could not issue token",
            )
        }
    }
}

/// @feature: remotes.status
#[tracing::instrument(skip_all)]
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

#[tracing::instrument(skip_all)]
async fn get_files(
    State(application_module): State<AppState>,
    vis: Visibility,
) -> Result<Json<Vec<File>>> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let db_files =
        dao::select_all_files_excluding_tags(&mut conn, vis.user_id(), vis.hidden_tags()).await?;
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

#[tracing::instrument(skip_all)]
async fn update_file(
    State(application_module): State<AppState>,
    vis: Visibility,
    Json(file): Json<File>,
) -> Result<Json<File>> {
    // The *stored* file must be visible to this request before any mutation;
    // hidden files 404 like missing ones.
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    if visible_file(&mut conn, &vis, &file.guid).await?.is_none() {
        return Err(Error::FileNotFound(file.guid.clone()));
    }
    drop(conn);
    application_module
        .db_client()
        .await
        .update_file(file.clone())
        .await?;
    Ok(Json(file))
}

/// @feature: tags.list
#[tracing::instrument(skip_all)]
async fn get_files_tags(
    State(application_module): State<AppState>,
    vis: Visibility,
) -> Result<Json<Vec<String>>> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let tags = dao::select_all_distinct_tags_excluding(&mut conn, vis.hidden_tags()).await?;
    Ok(Json(tags))
}

#[tracing::instrument(skip_all)]
async fn get_file(
    AxumPath(guid): AxumPath<String>,
    State(application_module): State<AppState>,
    vis: Visibility,
) -> Result<Response> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let Some((file, tags)) = visible_file(&mut conn, &vis, &guid).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let has_cover = dao::cover_exists(&mut conn, &file.fingerprint).await?;
    let mut api_file: File = (file, tags).into();
    api_file.has_cover = has_cover;
    Ok(Json(api_file).into_response())
}

#[tracing::instrument(skip_all)]
async fn get_file_tags(
    AxumPath(guid): AxumPath<String>,
    State(application_module): State<AppState>,
    vis: Visibility,
) -> Result<Json<Vec<String>>> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let Some((_, tags)) = visible_file(&mut conn, &vis, &guid).await? else {
        return Ok(Json(vec![]));
    };
    Ok(Json(tags.into_iter().map(|t| t.tag).collect()))
}

/// @feature: tags.add
#[tracing::instrument(skip_all)]
async fn post_file_tags(
    AxumPath(guid): AxumPath<String>,
    State(application_module): State<AppState>,
    vis: Visibility,
    Json(tags): Json<Vec<String>>,
) -> Result<Json<Vec<String>>> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let Some((file, _)) = visible_file(&mut conn, &vis, &guid).await? else {
        return Ok(Json(vec![]));
    };
    let content_tags = tags
        .into_iter()
        .map(|tag| crate::db::models::ContentTag::new(file.fingerprint.clone(), tag))
        .collect();
    dao::upsert_many_content_tags(&mut conn, content_tags).await?;
    let updated = dao::select_content_tags_by_fingerprint(&mut conn, &file.fingerprint).await?;
    Ok(Json(updated.into_iter().map(|t| t.tag).collect()))
}

/// @feature: tags.remove
#[tracing::instrument(skip_all)]
async fn delete_file_tags(
    AxumPath(guid): AxumPath<String>,
    State(application_module): State<AppState>,
    vis: Visibility,
    Json(tags): Json<Vec<String>>,
) -> Result<Json<Vec<String>>> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let Some((file, _)) = visible_file(&mut conn, &vis, &guid).await? else {
        return Ok(Json(vec![]));
    };
    dao::delete_content_tags(&mut conn, &file.fingerprint, tags).await?;
    let updated = dao::select_content_tags_by_fingerprint(&mut conn, &file.fingerprint).await?;
    Ok(Json(updated.into_iter().map(|t| t.tag).collect()))
}

#[tracing::instrument(skip_all)]
async fn download_file(
    AxumPath((guid, file_name)): AxumPath<(String, String)>,
    State(application_module): State<AppState>,
    vis: Visibility,
) -> Result<Response> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let Some((file, _)) = visible_file(&mut conn, &vis, &guid).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    if !file_name.ends_with(&file.type_.to_lowercase()) {
        tracing::error!(
            "Incorrect file extension on `{file_name}`, expected `{}`",
            file.type_
        );
        return Ok(StatusCode::NOT_FOUND.into_response());
    }

    let content_type = content_type_for_extension(&file.type_)?;
    let data = match (&file.archive_path, &file.archive_inner_path) {
        (Some(archive_path), Some(inner)) => {
            if !Path::new(archive_path).exists() {
                tracing::error!("Database out of sync, archive not found: {archive_path:?}");
                return Ok(StatusCode::NOT_FOUND.into_response());
            }
            let archive_path = archive_path.clone();
            let inner = inner.clone();
            tokio::task::spawn_blocking(move || {
                crate::scan::archive::extract_archive_member(Path::new(&archive_path), &inner)
            })
            .await
            .map_err(io::Error::other)??
        }
        _ => {
            let path = Path::new(&file.path);
            if !path.exists() {
                tracing::error!("Database out of sync, file not found: {path:?}");
                return Ok(StatusCode::NOT_FOUND.into_response());
            }
            tokio::fs::read(path).await?
        }
    };
    Ok(([(header::CONTENT_TYPE, content_type)], data).into_response())
}

#[tracing::instrument(skip_all)]
async fn get_file_cover(
    AxumPath(guid): AxumPath<String>,
    State(application_module): State<AppState>,
    vis: Visibility,
) -> Result<Response> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let Some((file, _)) = visible_file(&mut conn, &vis, &guid).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let Some((data, mime)) = dao::get_cover(&mut conn, &file.fingerprint).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    Ok(cover_response(data, mime))
}

fn require_pdf(file: &crate::db::models::File) -> Result<()> {
    if file.type_.to_lowercase() != "pdf" {
        return Err(Error::BadRequest(format!(
            "file type {} is not a PDF",
            file.type_
        )));
    }
    Ok(())
}

/// Resolve a readable filesystem path for a file, extracting archive members
/// to a temp file when needed. Drop the returned guard only once the path is
/// no longer read — it deletes the temp file.
async fn resolve_readable_path(
    file: &crate::db::models::File,
) -> Result<(PathBuf, Option<tempfile::NamedTempFile>)> {
    match (&file.archive_path, &file.archive_inner_path) {
        (Some(archive_path), Some(inner)) => {
            let tmp = crate::scan::scanner::extract_member_to_temp_file(
                PathBuf::from(archive_path),
                inner.clone(),
                file.type_.clone(),
            )
            .await?;
            let path = tmp.path().to_path_buf();
            Ok((path, Some(tmp)))
        }
        _ => Ok((PathBuf::from(&file.path), None)),
    }
}

#[derive(serde::Serialize)]
struct PdfPageCount {
    page_count: i32,
}

/// @feature: documents.change_thumbnail
#[tracing::instrument(skip_all)]
async fn get_pdf_page_count(
    AxumPath(guid): AxumPath<String>,
    State(application_module): State<AppState>,
    vis: Visibility,
) -> Result<Json<PdfPageCount>> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let Some((file, _)) = visible_file(&mut conn, &vis, &guid).await? else {
        return Err(Error::FileNotFound(guid));
    };
    require_pdf(&file)?;
    drop(conn);

    let (path, _guard) = resolve_readable_path(&file).await?;
    let page_count = tokio::task::spawn_blocking(move || crate::scan::cover::pdf_page_count(&path))
        .await
        .map_err(io::Error::other)?
        .ok_or_else(|| Error::BadRequest("could not read PDF page count".into()))?;
    Ok(Json(PdfPageCount { page_count }))
}

#[derive(serde::Deserialize)]
struct PdfPagePreviewQuery {
    #[serde(default)]
    trim: bool,
    #[serde(default = "default_preview_size")]
    size: String,
}

fn default_preview_size() -> String {
    "large".to_string()
}

/// @feature: documents.change_thumbnail
#[tracing::instrument(skip_all)]
async fn get_pdf_page_preview(
    AxumPath((guid, index)): AxumPath<(String, i32)>,
    State(application_module): State<AppState>,
    vis: Visibility,
    Query(query): Query<PdfPagePreviewQuery>,
) -> Result<Response> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let Some((file, _)) = visible_file(&mut conn, &vis, &guid).await? else {
        return Err(Error::FileNotFound(guid));
    };
    require_pdf(&file)?;
    drop(conn);

    let max_dim = if query.size == "thumb" { 200 } else { 800 };
    let trim = query.trim;
    let (path, _guard) = resolve_readable_path(&file).await?;
    let data = tokio::task::spawn_blocking(move || {
        let img = crate::scan::cover::render_pdf_page(&path, index, max_dim)?;
        let img = if trim {
            crate::scan::cover::trim_whitespace(&img)
        } else {
            img
        };
        crate::scan::cover::encode_page_webp(&img)
    })
    .await
    .map_err(io::Error::other)?
    .ok_or_else(|| Error::BadRequest("could not render page".into()))?;
    Ok(cover_response(data, "image/webp".to_string()))
}

#[derive(serde::Deserialize)]
struct SetPdfThumbnailRequest {
    #[serde(default)]
    trim: bool,
}

/// @feature: documents.change_thumbnail
#[tracing::instrument(skip_all)]
async fn post_pdf_page_thumbnail(
    AxumPath((guid, index)): AxumPath<(String, i32)>,
    State(application_module): State<AppState>,
    vis: Visibility,
    Json(body): Json<SetPdfThumbnailRequest>,
) -> Result<Json<ApiDocument>> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let Some((file, _)) = visible_file(&mut conn, &vis, &guid).await? else {
        return Err(Error::FileNotFound(guid));
    };
    require_pdf(&file)?;

    let trim = body.trim;
    let (path, _guard) = resolve_readable_path(&file).await?;
    let (data, mime) = tokio::task::spawn_blocking(move || {
        let img = crate::scan::cover::render_pdf_page(&path, index, 800)?;
        let img = if trim {
            crate::scan::cover::trim_whitespace(&img)
        } else {
            img
        };
        crate::scan::cover::encode_page_webp(&img).map(|data| (data, "image/webp".to_string()))
    })
    .await
    .map_err(io::Error::other)?
    .ok_or_else(|| Error::BadRequest("could not render page".into()))?;

    dao::set_custom_cover(
        &mut conn,
        &file.fingerprint,
        index as i64,
        trim,
        &data,
        &mime,
    )
    .await?;

    let doc = dao::ensure_document_for_fingerprint(&mut conn, &file.fingerprint).await?;
    let doc_row = dao::select_document_by_guid(&mut conn, &doc.guid)
        .await?
        .expect("document must exist after ensure_document_for_fingerprint");
    dao::set_selected_cover_fingerprint(&mut conn, doc_row.id, &file.fingerprint).await?;

    let updated = dao::select_api_document_by_guid(&mut conn, &doc.guid)
        .await?
        .expect("document must exist after upsert");
    Ok(Json(updated))
}

/// @feature: sources.delete
#[tracing::instrument(skip_all)]
async fn delete_file(
    AxumPath(guid): AxumPath<String>,
    State(application_module): State<AppState>,
    vis: Visibility,
) -> Result<()> {
    let guid = guid.as_str();
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    if visible_file(&mut conn, &vis, guid).await?.is_none() {
        return Err(Error::FileNotFound(guid.to_string()));
    }
    drop(conn);
    let db_client = application_module.db_client().await;
    let file = db_client.get_file(guid).await?;
    if let Some(ref file) = file {
        db_client.delete_file(file.clone()).await?;
        Ok(())
    } else {
        Err(Error::FileNotFound(guid.to_string()))
    }
}

/// @feature: sources.send_to_client
#[tracing::instrument(skip_all)]
async fn upload_file(
    State(application_module): State<AppState>,
    user: AuthorizedUser,
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
    let result = dao::select_file_by_path(
        &mut conn,
        &user.user_id,
        &canonical_path_string(&target_file),
    )
    .await?
    .ok_or_else(|| {
        Error::Scan("file not recorded after scan; server may be in dry-run mode".to_string())
    })?;
    Ok(Json((result, vec![]).into()))
}

#[tracing::instrument(skip_all)]
async fn get_reading_state(
    AxumPath(fingerprint): AxumPath<String>,
    State(application_module): State<AppState>,
    vis: Visibility,
) -> Result<Response> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    if !fingerprint_visible(&mut conn, &vis, &fingerprint).await? {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    let state = dao::get_reading_state(&mut conn, vis.user_id(), &fingerprint).await?;
    Ok(match state {
        Some(state) => Json(state).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    })
}

/// @feature: reading.progress
#[tracing::instrument(skip_all)]
async fn put_reading_state(
    State(application_module): State<AppState>,
    vis: Visibility,
    Json(state): Json<ReadingState>,
) -> Result<Json<ReadingState>> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    if !fingerprint_visible(&mut conn, &vis, &state.fingerprint).await? {
        return Err(Error::FileNotFound(state.fingerprint.clone()));
    }
    let result = dao::upsert_reading_state(&mut conn, vis.user_id(), state).await?;
    Ok(Json(result))
}

#[derive(serde::Deserialize)]
struct ReadingStatusRequest {
    status: ReadingStatus,
}

/// @feature: reading.status
#[tracing::instrument(skip_all)]
async fn put_reading_status(
    AxumPath(fingerprint): AxumPath<String>,
    State(application_module): State<AppState>,
    vis: Visibility,
    Json(req): Json<ReadingStatusRequest>,
) -> Result<()> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    if !fingerprint_visible(&mut conn, &vis, &fingerprint).await? {
        return Err(Error::FileNotFound(fingerprint.clone()));
    }
    dao::update_reading_status_only(&mut conn, vis.user_id(), &fingerprint, req.status.into())
        .await?;
    Ok(())
}

// ─── Document routes ──────────────────────────────────────────────────────────

/// @feature: documents.list
#[tracing::instrument(skip_all)]
async fn get_documents(
    State(application_module): State<AppState>,
    vis: Visibility,
) -> Result<Json<Vec<ApiDocument>>> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let docs = dao::select_all_api_documents(&mut conn).await?;
    let mut visible = Vec::with_capacity(docs.len());
    for doc in docs {
        if document_visible(&mut conn, &vis, &doc).await? {
            visible.push(doc);
        }
    }
    Ok(Json(visible))
}

/// @feature: documents.detail_view
#[tracing::instrument(skip_all)]
async fn get_document(
    AxumPath(guid): AxumPath<String>,
    State(application_module): State<AppState>,
    vis: Visibility,
) -> Result<Response> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let doc = dao::select_api_document_by_guid(&mut conn, &guid).await?;
    Ok(match doc {
        Some(doc) if document_visible(&mut conn, &vis, &doc).await? => Json(doc).into_response(),
        _ => StatusCode::NOT_FOUND.into_response(),
    })
}

/// @feature: documents.cover_display
#[tracing::instrument(skip_all)]
async fn get_document_cover(
    AxumPath(guid): AxumPath<String>,
    State(application_module): State<AppState>,
    vis: Visibility,
) -> Result<Response> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let Some(api_doc) = dao::select_api_document_by_guid(&mut conn, &guid).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    if !document_visible(&mut conn, &vis, &api_doc).await? {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
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
#[tracing::instrument(skip_all)]
async fn put_document_metadata(
    AxumPath(guid): AxumPath<String>,
    State(application_module): State<AppState>,
    vis: Visibility,
    Json(meta): Json<DocumentMeta>,
) -> Result<Json<ApiDocument>> {
    let guid = guid.as_str();
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;

    let api_doc = dao::select_api_document_by_guid(&mut conn, guid)
        .await?
        .ok_or_else(|| Error::FileNotFound(guid.to_string()))?;
    if !document_visible(&mut conn, &vis, &api_doc).await? {
        return Err(Error::FileNotFound(guid.to_string()));
    }
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

#[tracing::instrument(skip_all)]
async fn ensure_document_for_file(
    AxumPath(guid): AxumPath<String>,
    State(application_module): State<AppState>,
    vis: Visibility,
) -> Result<Json<ApiDocument>> {
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    if visible_file(&mut conn, &vis, &guid).await?.is_none() {
        return Err(Error::FileNotFound(guid.clone()));
    }
    let doc = dao::ensure_document_for_file_guid(&mut conn, vis.user_id(), &guid).await?;
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

/// Look up a file by guid and apply the request's visibility policy. Hidden
/// files are indistinguishable from missing ones (`None`), so responses do not
/// reveal that a private file exists.
async fn visible_file(
    conn: &mut sqlx::SqliteConnection,
    vis: &Visibility,
    guid: &str,
) -> Result<Option<(crate::db::models::File, Vec<crate::db::models::ContentTag>)>> {
    let Some(file) = dao::select_file_by_guid(conn, vis.user_id(), guid).await? else {
        return Ok(None);
    };
    let tags = dao::select_content_tags_by_fingerprint(conn, &file.fingerprint).await?;
    let tag_names: Vec<&str> = tags.iter().map(|t| t.tag.as_str()).collect();
    if !vis.can_see(&tag_names) {
        return Ok(None);
    }
    Ok(Some((file, tags)))
}

/// Whether the content behind `fingerprint` is visible to this request.
async fn fingerprint_visible(
    conn: &mut sqlx::SqliteConnection,
    vis: &Visibility,
    fingerprint: &str,
) -> Result<bool> {
    let tags = dao::select_content_tags_by_fingerprint(conn, fingerprint).await?;
    let tag_names: Vec<&str> = tags.iter().map(|t| t.tag.as_str()).collect();
    Ok(vis.can_see(&tag_names))
}

/// Whether a document is visible: it has no files, or at least one of its
/// files is visible to this request.
async fn document_visible(
    conn: &mut sqlx::SqliteConnection,
    vis: &Visibility,
    doc: &ApiDocument,
) -> Result<bool> {
    if doc.file_guids.is_empty() {
        return Ok(true);
    }
    for guid in &doc.file_guids {
        if visible_file(conn, vis, guid).await?.is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// @feature: admin.scan
#[tracing::instrument(skip_all)]
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
#[tracing::instrument(skip_all)]
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
#[tracing::instrument(skip_all)]
async fn get_scan_directories(
    State(application_module): State<AppState>,
    user: AuthorizedUser,
) -> Result<Json<Vec<ScanDirectoryEntry>>> {
    require_owner(&user)?;
    let settings = application_module.settings().await;
    Ok(Json(list_scan_directories(&settings)))
}

/// @feature: admin.scan_directories
#[tracing::instrument(skip_all)]
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
#[tracing::instrument(skip_all)]
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
    /// Single-pass extraction of tar archive members during scans.
    #[serde(default = "default_tar_single_pass_dto")]
    tar_single_pass: bool,
    private_mode: bool,
    private_tags: Vec<String>,
    /// Origins allowed by CORS (empty = any).
    #[serde(default)]
    allowed_origins: Vec<String>,
    /// Maximum upload size in bytes (`null` = server default).
    #[serde(default)]
    max_upload_bytes: Option<u64>,
}

fn default_tar_single_pass_dto() -> bool {
    true
}

fn server_settings_dto(settings: &Settings) -> ServerSettingsDto {
    ServerSettingsDto {
        database_url: settings.database.url().display().to_string(),
        extensions: settings.scan.extensions.clone(),
        dry_run: settings.scan.dry_run,
        concurrency: settings.scan.concurrency,
        tar_single_pass: settings.scan.tar_single_pass,
        private_mode: settings.ui.private_mode(),
        private_tags: settings.ui.private_tags().to_vec(),
        allowed_origins: settings.server.allowed_origins.clone(),
        max_upload_bytes: settings.server.max_upload_bytes,
    }
}

/// @feature: admin.server_settings
#[tracing::instrument(skip_all)]
async fn get_settings(
    State(application_module): State<AppState>,
    user: AuthorizedUser,
) -> Result<Json<ServerSettingsDto>> {
    require_owner(&user)?;
    let settings = application_module.settings().await;
    Ok(Json(server_settings_dto(&settings)))
}

/// @feature: admin.server_settings
#[tracing::instrument(skip_all)]
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
            s.scan.tar_single_pass = dto.tar_single_pass;
            s.ui.set_private_mode(dto.private_mode);
            s.ui.set_private_tags(dto.private_tags);
            s.server.allowed_origins = dto.allowed_origins;
            s.server.max_upload_bytes = dto.max_upload_bytes;
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
#[tracing::instrument(skip_all)]
async fn get_users(
    State(application_module): State<AppState>,
    user: AuthorizedUser,
) -> Result<Json<Vec<UserDto>>> {
    require_owner(&user)?;
    let settings = application_module.settings().await;
    Ok(Json(list_users(&settings)))
}

/// @feature: admin.authorized_users
#[tracing::instrument(skip_all)]
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
#[tracing::instrument(skip_all)]
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
#[tracing::instrument(skip_all)]
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
#[tracing::instrument(skip_all)]
async fn post_merge_documents(
    State(application_module): State<AppState>,
    vis: Visibility,
    Json(req): Json<MergeDocumentsRequest>,
) -> Result<Json<ApiDocument>> {
    let pool = application_module.connection_pool().await;
    {
        // Every involved document must be visible to this request.
        let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
        for guid in std::iter::once(&req.winner_guid).chain(req.loser_guids.iter()) {
            if let Some(doc) = dao::select_api_document_by_guid(&mut conn, guid).await?
                && !document_visible(&mut conn, &vis, &doc).await?
            {
                return Err(Error::FileNotFound(guid.clone()));
            }
        }
    }
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
#[tracing::instrument(skip_all)]
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
        .filter(|c| c.enabled())
        .map(Catalog::resolve)
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
#[tracing::instrument(skip_all)]
async fn import_online_book(
    State(application_module): State<AppState>,
    user: AuthorizedUser,
    Json(req): Json<ImportOnlineBookRequest>,
) -> Result<Json<File>> {
    let download_folder = application_module.settings().await.server.download_folder;
    let path = download_book(&req.format, &req.title, &download_folder)
        .await
        .map_err(|e| Error::Scan(e.to_string()))?;
    application_module.scan(path.clone()).await?;
    let pool = application_module.connection_pool().await;
    let mut conn = pool.acquire().await.map_err(dao::Error::from)?;
    let result = dao::select_file_by_path(&mut conn, &user.user_id, &canonical_path_string(&path))
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

#[cfg(test)]
mod tests {
    use assert4rs::Assert;

    use super::*;

    #[test]
    fn detect_lan_ip_never_returns_loopback() {
        // Environment-dependent (no network → None), but if it does return
        // something, it must be a real LAN-routable address, not 127.0.0.1 —
        // otherwise it'd be pointless to add as an extra SAN alongside
        // `localhost`, which already covers loopback.
        if let Some(ip) = detect_lan_ip() {
            Assert::that(ip.is_loopback()).is(false);
        }
    }
}
