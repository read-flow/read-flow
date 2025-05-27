use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::models::User;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,      // Subject (user ID)
    pub exp: usize,       // Expiration time
    pub iat: usize,       // Issued at
    pub role: String,     // User role
    pub username: String, // Username
}

#[derive(Debug, Error)]
pub enum JwtError {
    #[error("JWT error: {0}")]
    JwtError(#[from] jsonwebtoken::errors::Error),
    #[error("Invalid token")]
    InvalidToken,
}

pub struct JwtService {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
}

impl JwtService {
    pub fn new(secret: &[u8]) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(secret),
            decoding_key: DecodingKey::from_secret(secret),
        }
    }

    pub fn generate_token(&self, user: &User, duration_hours: i64) -> Result<String, JwtError> {
        let now = Utc::now();
        let expires_at = now + Duration::hours(duration_hours);

        let claims = Claims {
            sub: user.id.to_string(),
            exp: expires_at.timestamp() as usize,
            iat: now.timestamp() as usize,
            role: user.role().to_str().to_string(),
            username: user.username.clone(),
        };

        encode(&Header::default(), &claims, &self.encoding_key).map_err(JwtError::from)
    }

    pub fn validate_token(&self, token: &str) -> Result<Claims, JwtError> {
        let validation = Validation::new(Algorithm::HS256);

        let token_data = decode::<Claims>(token, &self.decoding_key, &validation)?;

        Ok(token_data.claims)
    }
}
