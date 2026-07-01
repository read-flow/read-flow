//! Helpers for booting a real `read-flow-cli serve` process bound to an
//! OS-assigned port, for BDD/integration tests that need a network-reachable
//! backend (e.g. the cucumber-rs `RestDriver` in `cosmic`'s BDD harness).
//!
//! The PWA Playwright harness (`pwa/e2e/support/server.ts`) follows the same
//! boot recipe independently, since it can't link against this crate: write a
//! temp config + SQLite path, spawn `read-flow-cli serve` with
//! `READ_FLOW_PORT=0`, and parse the bound address from the
//! "Server listening on ..." stdout line.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use std::time::Instant;

use tokio::io::AsyncBufReadExt;
use tokio::io::BufReader;
use tokio::io::Lines;
use tokio::process::Child;
use tokio::process::ChildStdout;
use tokio::process::Command;

use crate::settings::HashedPassword;

const LAUNCH_MARKER: &str = "Server listening on ";

/// A running `read-flow-cli serve` instance against a fresh temp config and
/// SQLite database, with one authorized "owner" user. Killed and cleaned up
/// on drop.
pub struct TestServer {
    pub base_url: String,
    pub user: String,
    pub password: String,
    /// Kept alive so `kill_on_drop` tears the process down when `TestServer` drops.
    _child: Child,
    dir: PathBuf,
}

impl TestServer {
    pub async fn spawn(user: &str, password: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("read-flow-bdd-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("create temp dir for test server");

        let hash = HashedPassword::with_rounds(password, pbkdf2::Params::MIN_ROUNDS)
            .expect("hash test password");
        let config_path = dir.join("read-flow.toml");
        std::fs::write(
            &config_path,
            format!(
                "[database]\nurl = \"{db}\"\n\n\
                 [server]\ndownload_folder = \"{folder}\"\n\n\
                 [server.authorized_users.{user}]\npassword = \"{hash}\"\nroles = [\"owner\"]\n",
                db = dir.join("test.db").display(),
                folder = dir.display(),
            ),
        )
        .expect("write temp server config");

        let mut child = Command::new(read_flow_cli_path())
            .args([
                "--configuration-file",
                config_path.to_str().expect("temp config path is utf-8"),
                "serve",
            ])
            .env("READ_FLOW_PORT", "0")
            .env("READ_FLOW_ADDRESS", "127.0.0.1")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .expect("spawn read-flow-cli serve");

        let stdout = child.stdout.take().expect("piped stdout");
        let mut lines = BufReader::new(stdout).lines();
        let base_url = wait_for_launch_url(&mut lines).await;
        keep_draining(lines);
        wait_for_http(&format!("{base_url}/status")).await;

        Self {
            base_url,
            user: user.to_string(),
            password: password.to_string(),
            _child: child,
            dir,
        }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The compiled `read-flow-cli` binary lives alongside the test binary's
/// `deps/` directory in the target dir — `CARGO_BIN_EXE_*` isn't available
/// here since the binary belongs to a different workspace package.
fn read_flow_cli_path() -> PathBuf {
    let mut path = std::env::current_exe().expect("path to current test binary");
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    path.push("read-flow-cli");
    path
}

async fn wait_for_launch_url(lines: &mut Lines<BufReader<ChildStdout>>) -> String {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            let line = lines
                .next_line()
                .await
                .expect("read server stdout")
                .expect("server exited before printing its launch address");
            if let Some(idx) = line.find(LAUNCH_MARKER) {
                return line[idx + LAUNCH_MARKER.len()..].trim().to_string();
            }
        }
    })
    .await
    .expect("timed out waiting for the server to launch")
}

/// Keep reading the server's stdout for its whole lifetime so the pipe never
/// fills up and blocks a write on the server side. (Tracing goes to stderr,
/// but anything the server prints to stdout after the launch line is drained
/// here.)
fn keep_draining(mut lines: Lines<BufReader<ChildStdout>>) {
    tokio::spawn(async move { while lines.next_line().await.transpose().is_some() {} });
}

async fn wait_for_http(url: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if reqwest::get(url).await.is_ok() {
            return;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for {url} to respond");
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}
