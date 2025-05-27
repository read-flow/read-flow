use rocket::{
    Request, State, get,
    http::Status,
    post,
    response::{self, Responder},
    serde::{Deserialize, Serialize, json::Json},
};
use serde_json::json;

use crate::{
    ApplicationModule,
    auth::{ApiKey, AuthError, Role, User},
    server::authn::{AdminUser, AuthorizedUser},
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct RegisterRequest {
    username: String,
    password: String,
    email: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct ApiKeyRequest {
    name: String,
    scopes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct LoginResponse {
    token: String,
    user: User,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct ApiKeyResponse {
    key: String,
    name: String,
    scopes: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("Authorization error: {0}")]
    Authorization(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Internal server error: {0}")]
    Internal(String),
}

// Implement Responder for Error
impl<'r> Responder<'r, 'static> for Error {
    fn respond_to(self, request: &'r Request<'_>) -> response::Result<'static> {
        let status = match self {
            Error::Auth(_) => Status::Unauthorized,
            Error::Authorization(_) => Status::Forbidden,
            Error::BadRequest(_) => Status::BadRequest,
            Error::Internal(_) => Status::InternalServerError,
        };

        let body = Json(json!({
            "error": self.to_string()
        }));

        body.respond_to(request).map(|mut response| {
            response.set_status(status);
            response
        })
    }
}

impl From<AuthError> for Error {
    fn from(error: AuthError) -> Self {
        match error {
            AuthError::InvalidCredentials => Error::Auth("Invalid credentials".to_string()),
            AuthError::UserAlreadyExists => Error::BadRequest("User already exists".to_string()),
            AuthError::InsufficientPermissions => {
                Error::Authorization("Insufficient permissions".to_string())
            }
            _ => Error::Internal(error.to_string()),
        }
    }
}

type Result<T> = std::result::Result<T, Error>;

#[post("/auth/login", data = "<login>")]
pub async fn login(
    login: Json<LoginRequest>,
    application_module: &State<ApplicationModule>,
) -> Result<Json<LoginResponse>> {
    let auth_service = application_module.auth_service();
    let jwt_service = application_module.jwt_service();

    // Authenticate the user
    let user = auth_service
        .authenticate(&login.username, &login.password)
        .await?;

    // Generate a JWT token
    let token = jwt_service
        .generate_token(&user, 24) // 24 hours
        .map_err(|e| Error::Internal(e.to_string()))?;

    Ok(Json(LoginResponse { token, user }))
}

#[post("/auth/register", data = "<register>")]
pub async fn register(
    register: Json<RegisterRequest>,
    application_module: &State<ApplicationModule>,
) -> Result<Json<User>> {
    let auth_service = application_module.auth_service();

    // Register the user with 'read' role by default
    let user = auth_service
        .register_user(
            &register.username,
            &register.password,
            register.email.as_deref(),
            Role::Read,
        )
        .await?;

    Ok(Json(user))
}

#[post("/auth/api-keys", data = "<request>")]
pub async fn create_api_key(
    request: Json<ApiKeyRequest>,
    user: AuthorizedUser,
    application_module: &State<ApplicationModule>,
) -> Result<Json<ApiKeyResponse>> {
    let auth_service = application_module.auth_service();

    // Convert string scopes to Role enum
    let scopes = request
        .scopes
        .iter()
        .filter_map(|s| Role::from_str(s))
        .collect::<Vec<_>>();

    // Create the API key
    let api_key = auth_service
        .create_api_key(user.user.id, &request.name, scopes)
        .await?;

    Ok(Json(ApiKeyResponse {
        key: api_key.key.clone(),
        name: api_key.name.clone(),
        scopes: api_key.scopes_vec(),
    }))
}

#[get("/auth/api-keys")]
pub async fn list_api_keys(
    user: AuthorizedUser,
    application_module: &State<ApplicationModule>,
) -> Result<Json<Vec<ApiKey>>> {
    let auth_service = application_module.auth_service();

    // List API keys for the current user
    let api_keys = auth_service.list_api_keys(user.user.id).await?;

    Ok(Json(api_keys))
}

#[get("/auth/users")]
pub async fn list_users(
    user: AdminUser, // Only admins can list users
    application_module: &State<ApplicationModule>,
) -> Result<Json<Vec<User>>> {
    let auth_service = application_module.auth_service();

    // List all users (admin only)
    let users = auth_service.list_users(user.user.id).await?;

    Ok(Json(users))
}
