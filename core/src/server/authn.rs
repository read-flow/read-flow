// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

use axum::extract::FromRequestParts;
use axum::http::HeaderValue;
use axum::http::StatusCode;
use axum::http::header;
use axum::http::request::Parts;
use axum::response::IntoResponse;
use axum::response::Response;
use base64::Engine;

use crate::server::AppState;

/// Rejects Basic-auth attempts once too many *failures* accumulated recently.
///
/// Every endpoint accepts Basic credentials, and each verification costs an
/// Argon2/PBKDF2 hash — so an attacker can brute-force passwords (and burn
/// CPU) against any route, not just `/oauth/token`. Successful logins are
/// never throttled; only a global budget of recent failures gates further
/// attempts, which are rejected *before* the expensive hash runs.
///
/// The key is global, like the `/oauth/token` governor: per-IP keying needs
/// `ConnectInfo`, and a reverse proxy is the recommended per-IP path.
pub struct BasicAuthLimiter {
    window: Duration,
    max_failures: usize,
    failures: Mutex<VecDeque<Instant>>,
}

impl Default for BasicAuthLimiter {
    fn default() -> Self {
        Self {
            window: Duration::from_secs(60),
            max_failures: 10,
            failures: Mutex::new(VecDeque::new()),
        }
    }
}

impl BasicAuthLimiter {
    /// Whether another Basic-auth attempt may proceed right now.
    fn check(&self) -> bool {
        let mut failures = self.failures.lock().expect("limiter lock");
        let cutoff = Instant::now() - self.window;
        while failures.front().is_some_and(|t| *t < cutoff) {
            failures.pop_front();
        }
        failures.len() < self.max_failures
    }

    fn record_failure(&self) {
        let mut failures = self.failures.lock().expect("limiter lock");
        failures.push_back(Instant::now());
        // Bound memory: entries beyond the budget are irrelevant.
        while failures.len() > self.max_failures {
            failures.pop_front();
        }
    }
}

pub struct AuthorizedUser {
    pub user_id: String,
    pub roles: Vec<String>,
}

impl AuthorizedUser {
    pub fn has_role(&self, role: &str) -> bool {
        self.roles.iter().any(|r| r == role)
    }
}

/// @feature: remotes.private_mode
pub struct PrivateModeHeader(pub bool);

impl<S> FromRequestParts<S> for PrivateModeHeader
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let value = parts
            .headers
            .get("x-private-mode")
            .and_then(|v| v.to_str().ok())
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        Ok(PrivateModeHeader(value))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("expected an Authorization header")]
    MissingAuthorization,
    #[error("expected a single Authorization header, found '{0}'")]
    TooManyAuthorizationHeaders(usize),
    #[error("expected a Basic or Bearer token")]
    InvalidAuthType,
    #[error("invalid Basic authentication format")]
    InvalidBasicAuth,
    #[error("the presented credentials are invalid")]
    InvalidCredentials,
    #[error("the bearer token is invalid or expired")]
    InvalidToken,
    #[error("too many failed authentication attempts, try again later")]
    TooManyAttempts,
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status = match &self {
            Error::TooManyAuthorizationHeaders(_) => StatusCode::BAD_REQUEST,
            Error::TooManyAttempts => StatusCode::TOO_MANY_REQUESTS,
            Error::MissingAuthorization
            | Error::InvalidAuthType
            | Error::InvalidBasicAuth
            | Error::InvalidToken => StatusCode::UNAUTHORIZED,
            Error::InvalidCredentials => StatusCode::FORBIDDEN,
        };
        // RFC 6750 §3: advertise the Bearer scheme on 401s.
        let challenge = match &self {
            Error::InvalidToken => "Bearer error=\"invalid_token\"",
            _ => "Bearer",
        };
        let mut response = (status, self.to_string()).into_response();
        if status == StatusCode::UNAUTHORIZED {
            response.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                HeaderValue::from_static(challenge),
            );
        }
        response
    }
}

impl AuthorizedUser {
    pub(crate) fn extract_basic_auth(
        authorization_header: &str,
    ) -> Result<(String, String), Error> {
        if !authorization_header.to_lowercase().starts_with("basic ") {
            return Err(Error::InvalidAuthType);
        }

        let encoded_credentials = &authorization_header[6..]; // Remove "Basic "
        let engine = base64::engine::general_purpose::STANDARD;
        let decoded = engine
            .decode(encoded_credentials)
            .map_err(|_| Error::InvalidBasicAuth)?;

        let credentials = String::from_utf8(decoded).map_err(|_| Error::InvalidBasicAuth)?;

        match credentials.split_once(':') {
            Some((user_id, passphrase)) => Ok((user_id.to_string(), passphrase.to_string())),
            None => Err(Error::InvalidBasicAuth),
        }
    }

    fn extract_bearer_token(authorization_header: &str) -> Result<&str, Error> {
        match authorization_header.split_once(" ") {
            Some((bearer, token)) => {
                if bearer.to_lowercase() != "bearer" {
                    Err(Error::InvalidAuthType)
                } else {
                    Ok(token)
                }
            }
            None => Err(Error::InvalidAuthType),
        }
    }
}

impl FromRequestParts<AppState> for AuthorizedUser {
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let headers: Vec<_> = parts.headers.get_all("authorization").iter().collect();
        let authorization_header = match headers.as_slice() {
            [] => return Err(Error::MissingAuthorization),
            [single] => single.to_str().map_err(|_| Error::InvalidBasicAuth)?,
            many => return Err(Error::TooManyAuthorizationHeaders(many.len())),
        };

        // Try Basic authentication first (user_id:passphrase). This is the
        // slow path (Argon2/PBKDF2) — used for `/oauth/token` and simple
        // clients. The hash runs on the blocking pool so it cannot stall
        // tokio's async workers, and a failure budget rejects brute-force
        // storms before the hash is even attempted.
        if authorization_header.to_lowercase().starts_with("basic ") {
            if !state.basic_limiter().check() {
                return Err(Error::TooManyAttempts);
            }
            let settings = state.settings().await;
            let (user_id, passphrase) = Self::extract_basic_auth(authorization_header)?;
            let entry = settings.server.authorized_users.get(&user_id).cloned();
            let verified = tokio::task::spawn_blocking(move || match entry {
                Some(entry) => entry
                    .password()
                    .verify(&passphrase)
                    .is_ok()
                    .then(|| entry.roles().to_vec()),
                None => {
                    // Spend the same time as a real verify so timing doesn't
                    // reveal which usernames exist.
                    crate::settings::verify_dummy(&passphrase);
                    None
                }
            })
            .await
            .map_err(|_| Error::InvalidCredentials)?;
            match verified {
                Some(roles) => Ok(AuthorizedUser { user_id, roles }),
                None => {
                    state.basic_limiter().record_failure();
                    Err(Error::InvalidCredentials)
                }
            }
        }
        // Bearer: verify the JWT with the in-memory secret. Fast path — no DB,
        // no PBKDF2. Roles come straight from the token claims.
        else {
            let token = Self::extract_bearer_token(authorization_header)?;
            match state.tokens().verify(token) {
                Ok(claims) => Ok(AuthorizedUser {
                    user_id: claims.sub,
                    roles: claims.roles,
                }),
                Err(_) => Err(Error::InvalidToken),
            }
        }
    }
}
