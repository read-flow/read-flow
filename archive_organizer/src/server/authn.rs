use itertools::Itertools;
use rocket::{
    http::Status,
    request::{FromRequest, Outcome, Request},
};

use crate::{ApplicationModule, auth::User};

pub struct AuthorizedUser {
    pub user: User,
}

pub struct AdminUser {
    pub user: User,
}

pub struct WriteUser {
    pub user: User,
}

pub struct ReadUser {
    pub user: User,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("expected a single Authorization header, found '{0}'")]
    TooManyAuthorizationHeaders(usize),
    #[error("expected a bearer token")]
    NotABearerToken,
    #[error("expected a bearer token, but got '{0}'")]
    ExpectedBearerToken(String),
    #[error("the presented token is invalid")]
    InvalidToken,
    #[error("insufficient permissions")]
    InsufficientPermissions,
    #[error("database error: {0}")]
    DatabaseError(String),
    #[error("authentication error: {0}")]
    AuthError(String),
}

impl AuthorizedUser {
    fn extract_bearer_token(authorization_header: &str) -> Result<&str, Error> {
        match authorization_header.split_once(" ") {
            Some((bearer, token)) => {
                if bearer.to_lowercase() != "bearer" {
                    Err(Error::ExpectedBearerToken(bearer.to_owned()))
                } else {
                    Ok(token)
                }
            }
            None => Err(Error::NotABearerToken),
        }
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for AuthorizedUser {
    type Error = Error;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let application_module = request
            .rocket()
            .state::<ApplicationModule>()
            .expect("ApplicationModule should exist");

        let settings = application_module.settings.clone();

        let authorization_header = request
            .headers()
            .get("authorization")
            .at_most_one()
            .map_err(|error| Error::TooManyAuthorizationHeaders(error.count()));

        match authorization_header {
            Ok(Some(authorization_header)) => {
                match Self::extract_bearer_token(authorization_header) {
                    Ok(token) => {
                        // First try JWT validation
                        let jwt_service = application_module.jwt_service();
                        match jwt_service.validate_token(token) {
                            Ok(claims) => {
                                // Get user from database to ensure it still exists and has correct permissions
                                let user_id = claims.sub.parse::<i32>().unwrap_or(0);
                                let _auth_service = application_module.auth_service();

                                // For now, just return success with the user ID
                                // In a real implementation, we would fetch the user from the database
                                let user = User {
                                    id: user_id,
                                    username: claims.username,
                                    password_hash: "".to_string(), // Not needed for authorization
                                    email: None,
                                    role: claims.role,
                                    created_at: chrono::Utc::now().naive_utc(),
                                    last_login: None,
                                };

                                Outcome::Success(AuthorizedUser { user })
                            }
                            Err(_) => {
                                // Fall back to API key validation
                                let auth_service = application_module.auth_service();
                                match auth_service.validate_api_key(token).await {
                                    Ok((_, user)) => Outcome::Success(AuthorizedUser { user }),
                                    Err(_) => {
                                        // Finally, fall back to legacy token validation
                                        if settings
                                            .server
                                            .authorization_tokens
                                            .contains(&token.to_owned())
                                        {
                                            // Create a default admin user for legacy tokens
                                            let user = User {
                                                id: 0,
                                                username: "legacy_admin".to_string(),
                                                password_hash: "".to_string(),
                                                email: None,
                                                role: "admin".to_string(),
                                                created_at: chrono::Utc::now().naive_utc(),
                                                last_login: None,
                                            };
                                            Outcome::Success(AuthorizedUser { user })
                                        } else {
                                            Outcome::Error((Status::Forbidden, Error::InvalidToken))
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(error) => Outcome::Error((Status::Unauthorized, error)),
                }
            }
            Ok(None) => Outcome::Forward(Status::Unauthorized),
            Err(error) => Outcome::Error((Status::BadRequest, error)),
        }
    }
}

// Role-specific request guards

#[rocket::async_trait]
impl<'r> FromRequest<'r> for AdminUser {
    type Error = Error;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        match AuthorizedUser::from_request(request).await {
            Outcome::Success(authorized) => {
                if authorized.user.role().can_admin() {
                    Outcome::Success(AdminUser {
                        user: authorized.user,
                    })
                } else {
                    Outcome::Error((Status::Forbidden, Error::InsufficientPermissions))
                }
            }
            Outcome::Error(e) => Outcome::Error(e),
            Outcome::Forward(status) => Outcome::Forward(status),
        }
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for WriteUser {
    type Error = Error;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        match AuthorizedUser::from_request(request).await {
            Outcome::Success(authorized) => {
                if authorized.user.role().can_write() {
                    Outcome::Success(WriteUser {
                        user: authorized.user,
                    })
                } else {
                    Outcome::Error((Status::Forbidden, Error::InsufficientPermissions))
                }
            }
            Outcome::Error(e) => Outcome::Error(e),
            Outcome::Forward(status) => Outcome::Forward(status),
        }
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for ReadUser {
    type Error = Error;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        match AuthorizedUser::from_request(request).await {
            Outcome::Success(authorized) => {
                if authorized.user.role().can_read() {
                    Outcome::Success(ReadUser {
                        user: authorized.user,
                    })
                } else {
                    Outcome::Error((Status::Forbidden, Error::InsufficientPermissions))
                }
            }
            Outcome::Error(e) => Outcome::Error(e),
            Outcome::Forward(status) => Outcome::Forward(status),
        }
    }
}
