use argon2::{self, password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString}, Argon2};
use chrono::Utc;
use diesel::prelude::*;
use rand::{thread_rng, Rng};
use rand::distributions::Alphanumeric;
use thiserror::Error;
use serde_json;

use crate::db::ConnectionPool;
use super::models::{User, NewUser, ApiKey, NewApiKey, Role, Scope};

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Database error: {0}")]
    DatabaseError(#[from] diesel::result::Error),
    #[error("Connection pool error: {0}")]
    PoolError(#[from] r2d2::Error),
    #[error("Invalid credentials")]
    InvalidCredentials,
    #[error("User already exists")]
    UserAlreadyExists,
    #[error("Password hashing failed")]
    PasswordHashingFailed,
    #[error("Invalid token")]
    InvalidToken,
    #[error("Insufficient permissions")]
    InsufficientPermissions,
    #[error("JSON serialization error: {0}")]
    JsonError(#[from] serde_json::Error),
}

pub struct AuthService {
    pool: ConnectionPool,
}

impl AuthService {
    pub fn new(pool: ConnectionPool) -> Self {
        Self { pool }
    }

    // Register a new user
    pub async fn register_user(&self, username_str: &str, password: &str, email_str: Option<&str>, new_role: Role) -> Result<User, AuthError> {
        use crate::db::schema::users::dsl::*;

        let mut conn = self.pool.get()?;

        // Check if user already exists
        let existing_user = users
            .filter(username.eq(username_str))
            .first::<User>(&mut conn)
            .optional()?;

        if existing_user.is_some() {
            return Err(AuthError::UserAlreadyExists);
        }

        // Hash the password
        let hashed_password = self.hash_password(password)?;

        // Create new user
        let new_user = NewUser {
            username: username_str.to_string(),
            password_hash: hashed_password,
            email: email_str.map(|e| e.to_string()),
            role: new_role.to_str().to_string(),
        };

        // Insert the user
        let user = diesel::insert_into(users)
            .values(&new_user)
            .get_result::<User>(&mut conn)?;

        Ok(user)
    }

    // Authenticate a user
    pub async fn authenticate(&self, username_str: &str, password: &str) -> Result<User, AuthError> {
        use crate::db::schema::users::dsl::*;

        let mut conn = self.pool.get()?;

        // Find the user
        let user = users
            .filter(username.eq(username_str))
            .first::<User>(&mut conn)
            .optional()?
            .ok_or(AuthError::InvalidCredentials)?;

        // Verify password
        if !self.verify_password(password, &user.password_hash)? {
            return Err(AuthError::InvalidCredentials);
        }

        // Update last login time
        diesel::update(&user)
            .set(last_login.eq(Utc::now().naive_utc()))
            .execute(&mut conn)?;

        Ok(user)
    }

    // Create a new API key for a user
    pub async fn create_api_key(&self, user_id_param: i32, name_str: &str, scopes_vec: Vec<Scope>) -> Result<ApiKey, AuthError> {
        use crate::db::schema::api_keys::dsl::*;

        let mut conn = self.pool.get()?;

        // Generate a random API key
        let key_str: String = thread_rng()
            .sample_iter(&Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();

        // Convert scopes to strings and serialize to JSON
        let scope_strings: Vec<String> = scopes_vec.iter().map(|s| s.to_str().to_string()).collect();
        let scopes_json = serde_json::to_string(&scope_strings)?;

        // Create new API key
        let new_key = NewApiKey {
            key: key_str,
            name: name_str.to_string(),
            user_id: user_id_param,
            scopes: scopes_json,
        };

        // Insert the API key
        let api_key = diesel::insert_into(api_keys)
            .values(&new_key)
            .get_result::<ApiKey>(&mut conn)?;

        Ok(api_key)
    }

    // Validate an API key
    pub async fn validate_api_key(&self, key_str: &str) -> Result<(ApiKey, User), AuthError> {
        use crate::db::schema::api_keys::dsl::*;
        use crate::db::schema::users::dsl::users;

        let mut conn = self.pool.get()?;

        // Find the API key
        let api_key = api_keys
            .filter(key.eq(key_str))
            .first::<ApiKey>(&mut conn)
            .optional()?
            .ok_or(AuthError::InvalidToken)?;

        // Check if expired
        if let Some(exp) = api_key.expires_at {
            if exp < Utc::now().naive_utc() {
                return Err(AuthError::InvalidToken);
            }
        }

        // Update last used time
        diesel::update(&api_key)
            .set(last_used.eq(Utc::now().naive_utc()))
            .execute(&mut conn)?;

        // Get the associated user
        let user = users
            .find(api_key.user_id)
            .first::<User>(&mut conn)?;

        Ok((api_key, user))
    }

    // List API keys for a user
    pub async fn list_api_keys(&self, user_id_param: i32) -> Result<Vec<ApiKey>, AuthError> {
        use crate::db::schema::api_keys::dsl::*;

        let mut conn = self.pool.get()?;

        let keys = api_keys
            .filter(user_id.eq(user_id_param))
            .load::<ApiKey>(&mut conn)?;

        Ok(keys)
    }

    // Delete an API key
    pub async fn delete_api_key(&self, key_id: i32, requesting_user_id: i32) -> Result<(), AuthError> {
        use crate::db::schema::api_keys::dsl::*;
        use crate::db::schema::users::dsl::users;

        let mut conn = self.pool.get()?;

        // Get the API key
        let api_key = api_keys
            .find(key_id)
            .first::<ApiKey>(&mut conn)?;

        // Check if the user owns this key or is an admin
        let requesting_user = users
            .find(requesting_user_id)
            .first::<User>(&mut conn)?;

        if api_key.user_id != requesting_user_id && !requesting_user.role().can_admin() {
            return Err(AuthError::InsufficientPermissions);
        }

        // Delete the key
        diesel::delete(api_keys.find(key_id))
            .execute(&mut conn)?;

        Ok(())
    }

    // List all users (admin only)
    pub async fn list_users(&self, requesting_user_id: i32) -> Result<Vec<User>, AuthError> {
        use crate::db::schema::users::dsl::*;

        let mut conn = self.pool.get()?;

        // Check if requesting user is admin
        let requesting_user = users
            .find(requesting_user_id)
            .first::<User>(&mut conn)?;

        if !requesting_user.role().can_admin() {
            return Err(AuthError::InsufficientPermissions);
        }

        // Get all users
        let all_users = users
            .load::<User>(&mut conn)?;

        Ok(all_users)
    }

    // Update user role (admin only)
    pub async fn update_user_role(&self, user_id: i32, new_role: Role, requesting_user_id: i32) -> Result<User, AuthError> {
        use crate::db::schema::users::dsl::*;

        let mut conn = self.pool.get()?;

        // Check if requesting user is admin
        let requesting_user = users
            .find(requesting_user_id)
            .first::<User>(&mut conn)?;

        if !requesting_user.role().can_admin() {
            return Err(AuthError::InsufficientPermissions);
        }

        // Update the user's role
        let updated_user = diesel::update(users.find(user_id))
            .set(role.eq(new_role.to_str()))
            .get_result::<User>(&mut conn)?;

        Ok(updated_user)
    }

    // Helper methods for password hashing
    fn hash_password(&self, password: &str) -> Result<String, AuthError> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();

        argon2.hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|_| AuthError::PasswordHashingFailed)
    }

    fn verify_password(&self, password: &str, hash: &str) -> Result<bool, AuthError> {
        let parsed_hash = PasswordHash::new(hash)
            .map_err(|_| AuthError::PasswordHashingFailed)?;

        Ok(Argon2::default().verify_password(password.as_bytes(), &parsed_hash).is_ok())
    }
}
