use itertools::Itertools;
use rocket::{
    http::Status,
    request::{FromRequest, Outcome, Request},
};

pub struct AuthorizedUser;

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
}

impl AuthorizedUser {
    fn extract_bearer_token(authorization_header: &str) -> Result<&str, Error> {
        match authorization_header.split_once(" ") {
            Some((bearer, token)) => {
                if bearer != "bearer" {
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
        let authorization_header = request
            .headers()
            .get("authorization")
            .at_most_one()
            .map_err(|error| Error::TooManyAuthorizationHeaders(error.count()));

        match authorization_header {
            Ok(Some(authorization_header)) => {
                match Self::extract_bearer_token(authorization_header) {
                    // TODO: validate token against some registry
                    Ok(token) => match token {
                        "secret" => Outcome::Success(AuthorizedUser),
                        _ => Outcome::Error((Status::Forbidden, Error::InvalidToken)),
                    },
                    Err(error) => Outcome::Error((Status::Unauthorized, error)),
                }
            }
            Ok(None) => Outcome::Forward(Status::Unauthorized),
            Err(error) => Outcome::Error((Status::BadRequest, error)),
        }
    }
}
