// SPDX-License-Identifier: AGPL-3.0-or-later

//! Bearer-token issuance and verification.
//!
//! After the resource owner authenticates once with HTTP Basic (PBKDF2), the
//! server hands out a short-lived **JWT** (HS256) that carries their roles.
//! Subsequent requests present it as `Authorization: Bearer <jwt>` and it is
//! verified with a cheap HMAC check — no database, no PBKDF2 per call.
//!
//! The signing secret is random per process ("ephemeral"): tokens are
//! invalidated on restart, and clients silently re-authenticate with their
//! cached password on the next 401.
//!
//! The claim shape follows the JWT registered-claim conventions (`iss`, `sub`,
//! `iat`, `exp`) plus a `roles` array (today's authorization source) and a
//! space-delimited `scope` mirroring the roles (OAuth-native, ready for
//! scope-based authorization later).

use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use jsonwebtoken::Algorithm;
use jsonwebtoken::DecodingKey;
use jsonwebtoken::EncodingKey;
use jsonwebtoken::Header;
use jsonwebtoken::Validation;
use rand::TryRng;
use rand::rngs::SysRng;
use serde::Deserialize;
use serde::Serialize;

/// Token lifetime. Later: configurable via `[server].token_ttl_seconds`.
const DEFAULT_TOKEN_TTL: Duration = Duration::from_secs(3600);

/// Issuer identifier placed in (and validated against) the `iss` claim.
const ISSUER: &str = "read-flow";

/// JWT payload. `sub` is the user id; `roles` drives authorization; `scope`
/// mirrors the roles for OAuth compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub iss: String,
    pub sub: String,
    pub iat: u64,
    pub exp: u64,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub scope: String,
}

#[derive(Debug, thiserror::Error)]
pub enum TokenError {
    #[error("could not sign token: {0}")]
    Sign(jsonwebtoken::errors::Error),
    #[error("invalid token")]
    Invalid,
}

/// Signs and verifies access tokens with a process-local HS256 secret.
pub struct TokenService {
    encoding: EncodingKey,
    decoding: DecodingKey,
    validation: Validation,
    ttl: Duration,
}

impl TokenService {
    /// Create a service with a fresh random 256-bit secret and the default TTL.
    pub fn generate() -> Self {
        Self::with_ttl(DEFAULT_TOKEN_TTL)
    }

    pub fn with_ttl(ttl: Duration) -> Self {
        let mut secret = [0u8; 32];
        SysRng
            .try_fill_bytes(&mut secret)
            .expect("OS RNG unavailable");

        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_issuer(&[ISSUER]);
        // `exp` is required and validated by default; we don't use `aud` yet.
        validation.validate_aud = false;

        Self {
            encoding: EncodingKey::from_secret(&secret),
            decoding: DecodingKey::from_secret(&secret),
            validation,
            ttl,
        }
    }

    /// Seconds a freshly issued token remains valid (for `expires_in`).
    pub fn ttl_seconds(&self) -> u64 {
        self.ttl.as_secs()
    }

    /// Issue a signed access token for `user_id` carrying `roles`.
    pub fn issue(&self, user_id: &str, roles: &[String]) -> Result<String, TokenError> {
        let now = unix_now();
        let claims = Claims {
            iss: ISSUER.to_string(),
            sub: user_id.to_string(),
            iat: now,
            exp: now + self.ttl.as_secs(),
            roles: roles.to_vec(),
            scope: roles.join(" "),
        };
        jsonwebtoken::encode(&Header::new(Algorithm::HS256), &claims, &self.encoding)
            .map_err(TokenError::Sign)
    }

    /// Verify a token's signature and expiry, returning its claims.
    pub fn verify(&self, token: &str) -> Result<Claims, TokenError> {
        jsonwebtoken::decode::<Claims>(token, &self.decoding, &self.validation)
            .map(|data| data.claims)
            .map_err(|_| TokenError::Invalid)
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use assert4rs::Assert;

    use super::*;

    #[test]
    fn issue_then_verify_round_trips() {
        let service = TokenService::generate();
        let token = service
            .issue("alice", &["owner".to_string()])
            .expect("issue");
        let claims = service.verify(&token).expect("verify");
        Assert::that(claims.sub).is("alice");
        Assert::that(claims.roles).is_eq_to(vec!["owner".to_string()]);
        Assert::that(claims.scope).is("owner");
        Assert::that(claims.iss).is(ISSUER);
    }

    #[test]
    fn rejects_token_from_a_different_secret() {
        let a = TokenService::generate();
        let b = TokenService::generate();
        let token = a.issue("alice", &[]).expect("issue");
        assert!(b.verify(&token).is_err());
    }

    #[test]
    fn rejects_expired_token() {
        // Zero TTL → `exp == iat`, already expired (with default leeway the
        // token is at the boundary; use a clearly-past ttl via a manual claim).
        let service = TokenService::with_ttl(Duration::from_secs(0));
        let now = unix_now();
        // Well past the default 60s validation leeway.
        let claims = Claims {
            iss: ISSUER.to_string(),
            sub: "alice".to_string(),
            iat: now - 7200,
            exp: now - 3600,
            roles: vec![],
            scope: String::new(),
        };
        let token =
            jsonwebtoken::encode(&Header::new(Algorithm::HS256), &claims, &service.encoding)
                .expect("encode");
        assert!(service.verify(&token).is_err());
    }
}
