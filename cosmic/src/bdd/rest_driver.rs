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

    /// Owner-only `/settings` GET/PUT, manipulated as raw JSON — no need to
    /// duplicate the server's private `ServerSettingsDto` shape for one field.
    async fn get_settings(&self) -> serde_json::Value {
        self.client
            .get(format!("{}/settings", self.server.base_url))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .send()
            .await
            .expect("GET /settings")
            .json()
            .await
            .expect("parse settings JSON")
    }

    async fn put_settings(&self, dto: serde_json::Value) {
        let response = self
            .client
            .put(format!("{}/settings", self.server.base_url))
            .basic_auth(&self.server.user, Some(&self.server.password))
            .json(&dto)
            .send()
            .await
            .expect("PUT /settings");
        assert!(
            response.status().is_success(),
            "PUT /settings failed: {}",
            response.status()
        );
    }

    pub async fn enable_dry_run_and_save(&self) {
        let mut dto = self.get_settings().await;
        dto["dry_run"] = serde_json::Value::Bool(true);
        self.put_settings(dto).await;
    }

    pub async fn dry_run_is_enabled(&self) -> bool {
        self.get_settings().await["dry_run"]
            .as_bool()
            .expect("dry_run is a bool")
    }
}
