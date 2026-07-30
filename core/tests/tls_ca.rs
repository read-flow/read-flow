//! Regression tests for the local CA used to issue browser-trustable TLS
//! certs without a public hostname (see plan: "Transport security for the
//! self-hosted server"). A single long-lived CA root is generated once and
//! trusted on each client device; every leaf cert issued after that is
//! automatically trusted without a further per-cert trust step.
#![cfg(feature = "server")]

use std::sync::Arc;

use assert4rs::Assert;
use read_flow_core::server;
use rustls::RootCertStore;
use rustls::client::WebPkiServerVerifier;
use rustls::client::danger::ServerCertVerifier;
use rustls::pki_types::CertificateDer;
use rustls::pki_types::ServerName;
use rustls::pki_types::UnixTime;

/// `WebPkiServerVerifier` needs a process-level crypto provider installed.
/// Explicit here rather than relying on rustls's single-feature
/// auto-detection, which breaks the moment any other crate in the graph
/// enables a second backend. Must match the backend already used elsewhere
/// in the workspace (aws-lc-rs). Safe to call from every test: a second call
/// just returns `Err` because one's already installed.
fn ensure_crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

fn read_der(path: &std::path::Path) -> CertificateDer<'static> {
    let pem = std::fs::read_to_string(path).expect("read pem");
    CertificateDer::from(rustls_pemfile_certs(&pem))
}

/// Minimal single-cert PEM -> DER extraction (avoids pulling in rustls-pemfile
/// just for tests): strips the PEM armor and base64-decodes the body.
fn rustls_pemfile_certs(pem: &str) -> Vec<u8> {
    let body: String = pem
        .lines()
        .filter(|l| !l.starts_with("-----"))
        .collect::<Vec<_>>()
        .join("");
    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, body)
        .expect("base64 decode cert")
}

#[test]
fn generate_local_ca_writes_a_valid_ca_cert_and_key() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (ca_cert, ca_key) = server::generate_local_ca(dir.path()).expect("generate ca");
    Assert::that(ca_cert.exists()).is(true);
    Assert::that(ca_key.exists()).is(true);
}

#[tokio::test]
async fn leaf_cert_signed_by_local_ca_chains_to_the_ca_for_its_sans() {
    ensure_crypto_provider();
    let dir = tempfile::tempdir().expect("temp dir");
    let (ca_cert_path, ca_key_path) = server::generate_local_ca(dir.path()).expect("generate ca");

    let (leaf_cert_path, leaf_key_path) = server::generate_ca_signed_cert(
        dir.path(),
        &ca_cert_path,
        &ca_key_path,
        vec!["localhost".to_string(), "192.168.1.50".to_string()],
    )
    .expect("issue leaf cert");
    Assert::that(leaf_cert_path.exists()).is(true);
    Assert::that(leaf_key_path.exists()).is(true);

    // The leaf/key pair must also be a valid rustls TLS config (same bar the
    // plain self-signed path is already held to).
    server::RustlsConfig::from_pem_file(&leaf_cert_path, &leaf_key_path)
        .await
        .expect("leaf cert/key load as a rustls config");

    // The real proof: a client that only trusts our CA root actually
    // validates the leaf as a genuine chain (signature + hostname), exactly
    // like a browser would — not just "the CA's name appears in the leaf".
    let mut roots = RootCertStore::empty();
    roots.add(read_der(&ca_cert_path)).expect("add ca root");
    let verifier = WebPkiServerVerifier::builder(Arc::new(roots))
        .build()
        .expect("build verifier");

    let leaf_der = read_der(&leaf_cert_path);
    let server_name = ServerName::try_from("localhost").expect("server name");
    verifier
        .verify_server_cert(&leaf_der, &[], &server_name, &[], UnixTime::now())
        .expect("leaf cert validates against the CA root for `localhost`");

    // A SAN the leaf was NOT issued for must not validate.
    let wrong_name = ServerName::try_from("evil.example").expect("server name");
    let result = verifier.verify_server_cert(&leaf_der, &[], &wrong_name, &[], UnixTime::now());
    Assert::that(result.is_err()).is(true);
}

#[test]
fn leaf_cert_is_not_itself_a_ca() {
    ensure_crypto_provider();
    // A leaf cert must not be usable to sign further certs — otherwise
    // trusting the leaf (instead of just the root) would let it mint new
    // trusted certs for arbitrary names.
    let dir = tempfile::tempdir().expect("temp dir");
    let (ca_cert_path, ca_key_path) = server::generate_local_ca(dir.path()).expect("generate ca");
    let (leaf_cert_path, _) = server::generate_ca_signed_cert(
        dir.path(),
        &ca_cert_path,
        &ca_key_path,
        vec!["localhost".to_string()],
    )
    .expect("issue leaf cert");

    let mut roots = RootCertStore::empty();
    roots
        .add(read_der(&leaf_cert_path))
        .expect("add leaf as root");
    let verifier = WebPkiServerVerifier::builder(Arc::new(roots))
        .build()
        .expect("build verifier");

    // Trying to validate the CA root itself against a "roots store" that
    // only contains the leaf must fail: the leaf has no CA basic constraint,
    // so nothing it "signs" (including the actual CA, which it did not sign)
    // can be treated as trusted through it.
    let ca_der = read_der(&ca_cert_path);
    let server_name = ServerName::try_from("localhost").expect("server name");
    let result = verifier.verify_server_cert(&ca_der, &[], &server_name, &[], UnixTime::now());
    Assert::that(result.is_err()).is(true);
}

#[test]
fn regenerating_a_leaf_cert_from_the_same_ca_does_not_require_re_trusting_the_ca() {
    ensure_crypto_provider();
    // Simulates the real workflow: the CA is generated once, then the leaf
    // is regenerated later (e.g. the bind address changed) without touching
    // the CA. A client that trusted the CA root once should still trust the
    // new leaf without any further action.
    let dir = tempfile::tempdir().expect("temp dir");
    let (ca_cert_path, ca_key_path) = server::generate_local_ca(dir.path()).expect("generate ca");

    let (first_leaf, _) = server::generate_ca_signed_cert(
        dir.path(),
        &ca_cert_path,
        &ca_key_path,
        vec!["localhost".to_string()],
    )
    .expect("issue first leaf");
    let (second_leaf, _) = server::generate_ca_signed_cert(
        dir.path(),
        &ca_cert_path,
        &ca_key_path,
        vec!["localhost".to_string(), "192.168.1.99".to_string()],
    )
    .expect("issue second leaf");

    let mut roots = RootCertStore::empty();
    roots.add(read_der(&ca_cert_path)).expect("add ca root");
    let verifier = WebPkiServerVerifier::builder(Arc::new(roots))
        .build()
        .expect("build verifier");

    let server_name = ServerName::try_from("localhost").expect("server name");
    verifier
        .verify_server_cert(
            &read_der(&first_leaf),
            &[],
            &server_name,
            &[],
            UnixTime::now(),
        )
        .expect("first leaf still trusted");
    verifier
        .verify_server_cert(
            &read_der(&second_leaf),
            &[],
            &server_name,
            &[],
            UnixTime::now(),
        )
        .expect("second leaf (regenerated) trusted without re-trusting the CA");
}

#[test]
fn generate_or_reuse_ca_signed_cert_reuses_an_existing_ca() {
    ensure_crypto_provider();
    let dir = tempfile::tempdir().expect("temp dir");

    let (first_leaf, _) =
        server::generate_or_reuse_ca_signed_cert(dir.path(), vec!["localhost".to_string()])
            .expect("first call generates a CA + leaf");
    let ca_pem_after_first = std::fs::read_to_string(server::ca_cert_path(dir.path())).unwrap();

    let (second_leaf, _) = server::generate_or_reuse_ca_signed_cert(
        dir.path(),
        vec!["localhost".to_string(), "192.168.1.42".to_string()],
    )
    .expect("second call reuses the CA");
    let ca_pem_after_second = std::fs::read_to_string(server::ca_cert_path(dir.path())).unwrap();

    // The CA itself must be byte-for-byte untouched by the second call — if
    // it were regenerated, every device that already trusted the first CA
    // root would silently stop being able to connect.
    Assert::that(ca_pem_after_second.as_str()).is(ca_pem_after_first.as_str());

    // Both leaves still validate against that one CA root.
    let mut roots = RootCertStore::empty();
    roots
        .add(read_der(&server::ca_cert_path(dir.path())))
        .expect("add ca root");
    let verifier = WebPkiServerVerifier::builder(Arc::new(roots))
        .build()
        .expect("build verifier");
    let server_name = ServerName::try_from("localhost").expect("server name");
    verifier
        .verify_server_cert(
            &read_der(&first_leaf),
            &[],
            &server_name,
            &[],
            UnixTime::now(),
        )
        .expect("first leaf trusted");
    verifier
        .verify_server_cert(
            &read_der(&second_leaf),
            &[],
            &server_name,
            &[],
            UnixTime::now(),
        )
        .expect("second leaf trusted via the same reused CA");
}

/// Minimal `read-flow.toml` for the router-level `/ca.pem` tests below.
/// `tls` is `None` when `cert_key` is `None`.
fn write_test_config(
    dir: &std::path::Path,
    cert_key: Option<(&std::path::Path, &std::path::Path)>,
) -> std::path::PathBuf {
    let db_path = dir.join("test.db");
    let download = dir.join("dl");
    std::fs::create_dir_all(&download).expect("download dir");

    let tls_section = match cert_key {
        Some((cert, key)) => format!(
            "\n[server.tls]\ncert = \"{}\"\nkey = \"{}\"\n",
            cert.display(),
            key.display()
        ),
        None => String::new(),
    };
    let config = format!(
        "[database]\nurl = \"{db}\"\n\n[server]\ndownload_folder = \"{dl}\"\nauthorized_users = {{}}\n{tls_section}",
        db = db_path.display(),
        dl = download.display(),
    );
    let config_path = dir.join("read-flow.toml");
    std::fs::write(&config_path, config).expect("write config");
    config_path
}

#[tokio::test]
async fn ca_pem_route_serves_the_ca_root_when_tls_is_configured() {
    use axum::body::Body;
    use axum::body::to_bytes;
    use axum::http::Request;
    use axum::http::StatusCode;
    use tower::ServiceExt;

    let dir = tempfile::tempdir().expect("temp dir");
    let (leaf_cert, leaf_key) =
        server::generate_or_reuse_ca_signed_cert(dir.path(), vec!["localhost".to_string()])
            .expect("generate ca + leaf");
    let config_path = write_test_config(dir.path(), Some((&leaf_cert, &leaf_key)));

    let router = server::build_app(config_path).await.expect("build router");
    let response = router
        .oneshot(
            Request::builder()
                .uri("/ca.pem")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");

    Assert::that(response.status()).is(StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let body = String::from_utf8(body.to_vec()).expect("utf8");
    Assert::that(body.as_str()).is(std::fs::read_to_string(server::ca_cert_path(dir.path()))
        .expect("read ca cert")
        .as_str());
}

#[tokio::test]
async fn ca_pem_route_404s_when_no_tls_is_configured() {
    use axum::body::Body;
    use axum::http::Request;
    use axum::http::StatusCode;
    use tower::ServiceExt;

    let dir = tempfile::tempdir().expect("temp dir");
    let config_path = write_test_config(dir.path(), None);

    let router = server::build_app(config_path).await.expect("build router");
    let response = router
        .oneshot(
            Request::builder()
                .uri("/ca.pem")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");

    Assert::that(response.status()).is(StatusCode::NOT_FOUND);
}
