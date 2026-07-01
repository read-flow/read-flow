use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::IntoResponse;
use axum::response::Response;
use base64::Engine;

use crate::server::AppState;

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
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status = match self {
            Error::TooManyAuthorizationHeaders(_) => StatusCode::BAD_REQUEST,
            Error::MissingAuthorization | Error::InvalidAuthType | Error::InvalidBasicAuth => {
                StatusCode::UNAUTHORIZED
            }
            Error::InvalidCredentials => StatusCode::FORBIDDEN,
        };
        (status, self.to_string()).into_response()
    }
}

impl AuthorizedUser {
    fn extract_basic_auth(authorization_header: &str) -> Result<(String, String), Error> {
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
        let settings = state.settings().await;

        let headers: Vec<_> = parts.headers.get_all("authorization").iter().collect();
        let authorization_header = match headers.as_slice() {
            [] => return Err(Error::MissingAuthorization),
            [single] => single.to_str().map_err(|_| Error::InvalidBasicAuth)?,
            many => return Err(Error::TooManyAuthorizationHeaders(many.len())),
        };

        // Try Basic authentication first (user_id:passphrase)
        if authorization_header.to_lowercase().starts_with("basic ") {
            let (user_id, passphrase) = Self::extract_basic_auth(authorization_header)?;
            match settings.server.authorized_users.get(&user_id) {
                Some(entry) if entry.password().verify(&passphrase).is_ok() => Ok(AuthorizedUser {
                    user_id,
                    roles: entry.roles().to_vec(),
                }),
                _ => Err(Error::InvalidCredentials),
            }
        }
        // Fall back to Bearer token authentication for backward compatibility
        else {
            let token = Self::extract_bearer_token(authorization_header)?;
            for (user_id, entry) in settings.server.authorized_users.iter() {
                if entry.password().verify(token).is_ok() {
                    return Ok(AuthorizedUser {
                        user_id: user_id.clone(),
                        roles: entry.roles().to_vec(),
                    });
                }
            }
            Err(Error::InvalidCredentials)
        }
    }
}
