//! Drives Gherkin steps against a real `read-flow-cli serve` process over
//! HTTP — the most direct mapping for REST-surfaced behavior. Reuses
//! `read_flow_core::test_support::TestServer`, the same boot recipe the PWA
//! Playwright harness follows independently (see `pwa/e2e/support/server.ts`).

use read_flow_core::test_support::TestServer;

pub const USER: &str = "alice";
pub const PASSWORD: &str = "correct-horse";

pub struct RestDriver {
    server: TestServer,
    client: reqwest::Client,
}

impl RestDriver {
    pub async fn new() -> Self {
        Self {
            server: TestServer::spawn(USER, PASSWORD).await,
            client: reqwest::Client::new(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.server.base_url
    }

    /// `GET /status` with the server's configured credentials — REST has no
    /// "add remote" concept, so the natural mapping for "a remote reports as
    /// reachable" is "the server's own status endpoint accepts these creds".
    pub async fn status_with(&self, user: &str, password: &str) -> bool {
        self.client
            .get(format!("{}/status", self.server.base_url))
            .basic_auth(user, Some(password))
            .send()
            .await
            .map(|response| response.status().is_success())
            .unwrap_or(false)
    }

    pub async fn status_is_healthy(&self) -> bool {
        self.status_with(&self.server.user, &self.server.password)
            .await
    }
}
