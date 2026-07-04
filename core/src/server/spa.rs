//! Serves the embedded Progressive Web App (the built `pwa/build`) as the
//! router's fallback, so a single `read-flow` server can host both the REST API
//! and the web UI at the same origin — no CORS, no mixed-content, works on a
//! plain-HTTP LAN. Enabled by the `embed-pwa` feature.

use axum::http::StatusCode;
use axum::http::Uri;
use axum::http::header;
use axum::response::IntoResponse;
use axum::response::Response;

#[derive(rust_embed::RustEmbed)]
#[folder = "../pwa/build"]
struct Assets;

/// Router fallback: serve the requested asset, or fall back to `index.html`
/// so client-side routes resolve (single-page app).
pub async fn handler(uri: Uri) -> Response {
    serve(uri.path())
}

fn serve(path: &str) -> Response {
    let trimmed = path.trim_start_matches('/');
    let candidate = if trimmed.is_empty() {
        "index.html"
    } else {
        trimmed
    };

    if let Some(asset) = Assets::get(candidate) {
        return respond(asset.metadata.mimetype(), asset.data.into_owned());
    }

    // No literal file. A path that looks like a file (has an extension) is
    // genuinely missing; anything else is a client-side route → app shell.
    if looks_like_file(candidate) {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    match Assets::get("index.html") {
        Some(index) => respond("text/html; charset=utf-8", index.data.into_owned()),
        None => (StatusCode::NOT_FOUND, "PWA assets not embedded").into_response(),
    }
}

fn respond(content_type: &str, body: Vec<u8>) -> Response {
    ([(header::CONTENT_TYPE, content_type.to_owned())], body).into_response()
}

/// A path whose final segment contains a `.` is treated as a concrete file
/// request; anything else is a single-page-app route.
fn looks_like_file(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .is_some_and(|segment| segment.contains('.'))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::looks_like_file;

    #[rstest]
    #[case("index.html", true)]
    #[case("assets/app.js", true)]
    #[case("icons/pwa-192x192.png", true)]
    #[case("library", false)]
    #[case("documents/some-guid", false)]
    #[case("", false)]
    fn classifies_paths(#[case] path: &str, #[case] expected: bool) {
        assert_eq!(looks_like_file(path), expected);
    }
}
