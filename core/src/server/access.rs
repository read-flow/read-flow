// SPDX-License-Identifier: AGPL-3.0-or-later

//! Centralized content-visibility policy for the HTTP API.
//!
//! Every handler that can expose (or mutate) library content extracts a
//! [`Visibility`] instead of combining [`AuthorizedUser`] and
//! [`PrivateModeHeader`] by hand. The policy is decided in exactly one place:
//!
//! * `x-private-mode: true` **and** the `owner` role → full visibility
//!   (nothing hidden). A non-owner sending the header is rejected with
//!   `403 Forbidden` before the handler body runs.
//! * otherwise → the configured `[ui].private_tags` are hidden, regardless of
//!   the server-side `ui.private_mode` toggle. That toggle is a *local GUI*
//!   viewing preference; remote clients must always request private content
//!   explicitly via the header so a desktop session left in private mode
//!   cannot leak hidden files to other API users.

use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::IntoResponse;
use axum::response::Response;

use crate::server::AppState;
use crate::server::authn::AuthorizedUser;
use crate::server::authn::PrivateModeHeader;

/// What the current request is allowed to see. Constructed via the axum
/// extractor impl; handlers use [`Visibility::can_see`] / [`Visibility::hidden_tags`]
/// to filter content. Extraction authenticates the request (any valid user).
pub struct Visibility {
    /// The authenticated user this request acts as. Reading state is stored
    /// per user id.
    user_id: String,
    /// Tags whose content is hidden from this request. Empty means full
    /// visibility (owner in private mode, or no private tags configured).
    hidden_tags: Vec<String>,
}

impl Visibility {
    /// The authenticated user id for this request.
    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    /// Tags hidden from this request, for SQL-side exclusion.
    pub fn hidden_tags(&self) -> &[String] {
        &self.hidden_tags
    }

    /// Whether content carrying `tags` is visible to this request.
    pub fn can_see<S: AsRef<str>>(&self, tags: &[S]) -> bool {
        !tags
            .iter()
            .any(|tag| self.hidden_tags.iter().any(|h| h == tag.as_ref()))
    }
}

impl FromRequestParts<AppState> for Visibility {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user = AuthorizedUser::from_request_parts(parts, state)
            .await
            .map_err(IntoResponse::into_response)?;
        let PrivateModeHeader(private_mode) = PrivateModeHeader::from_request_parts(parts, state)
            .await
            .map_err(IntoResponse::into_response)?;

        if private_mode {
            if !user.has_role("owner") {
                return Err((
                    StatusCode::FORBIDDEN,
                    "private mode access requires owner role",
                )
                    .into_response());
            }
            return Ok(Self {
                user_id: user.user_id,
                hidden_tags: Vec::new(),
            });
        }

        let settings = state.settings().await;
        Ok(Self {
            user_id: user.user_id,
            hidden_tags: settings.ui.private_tags().to_vec(),
        })
    }
}
