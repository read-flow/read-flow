use base64::Engine;
use itertools::Itertools;
use rocket::http::Status;
use rocket::request::FromRequest;
use rocket::request::Outcome;
use rocket::request::Request;

use crate::ApplicationModule;
use crate::server::SettingsProvider;

pub struct AuthorizedUser {
    pub user_id: String,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("expected a single Authorization header, found '{0}'")]
    TooManyAuthorizationHeaders(usize),
    #[error("expected a Basic or Bearer token")]
    InvalidAuthType,
    #[error("invalid Basic authentication format")]
    InvalidBasicAuth,
    #[error("the presented credentials are invalid")]
    InvalidCredentials,
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

#[rocket::async_trait]
impl<'r> FromRequest<'r> for AuthorizedUser {
    type Error = Error;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let application_module = request
            .rocket()
            .state::<ApplicationModule<SettingsProvider>>()
            .expect("ApplicationModule should exist");

        let settings = application_module.settings();

        let authorization_header = request
            .headers()
            .get("authorization")
            .at_most_one()
            .map_err(|error| Error::TooManyAuthorizationHeaders(error.count()));

        match authorization_header {
            Ok(Some(authorization_header)) => {
                // Try Basic authentication first (user_id:passphrase)
                if authorization_header.to_lowercase().starts_with("basic ") {
                    match Self::extract_basic_auth(authorization_header) {
                        Ok((user_id, passphrase)) => {
                            // Check against authorized_users hashmap
                            if let Some(stored_passphrase) =
                                settings.server.authorized_users.get(&user_id)
                            {
                                if stored_passphrase.verify(&passphrase).is_ok() {
                                    Outcome::Success(AuthorizedUser { user_id })
                                } else {
                                    Outcome::Error((Status::Forbidden, Error::InvalidCredentials))
                                }
                            } else {
                                Outcome::Error((Status::Forbidden, Error::InvalidCredentials))
                            }
                        }
                        Err(error) => Outcome::Error((Status::Unauthorized, error)),
                    }
                }
                // Fall back to Bearer token authentication for backward compatibility
                else {
                    match Self::extract_bearer_token(authorization_header) {
                        Ok(token) => {
                            // Check if token matches any stored passphrase (legacy support)
                            for (user_id, stored_passphrase) in
                                settings.server.authorized_users.iter()
                            {
                                if stored_passphrase.verify(token).is_ok() {
                                    return Outcome::Success(AuthorizedUser {
                                        user_id: user_id.clone(),
                                    });
                                }
                            }
                            Outcome::Error((Status::Forbidden, Error::InvalidCredentials))
                        }
                        Err(error) => Outcome::Error((Status::Unauthorized, error)),
                    }
                }
            }
            Ok(None) => Outcome::Forward(Status::Unauthorized),
            Err(error) => Outcome::Error((Status::BadRequest, error)),
        }
    }
}
