//! Shared seed-data fixtures for scenarios needing real `Document` rows
//! (`tags_list` and onward — see its feature file's doc comment for why a
//! minimal *valid* EPUB is required: the scanner only creates a `Document`
//! when metadata extraction succeeds).

/// A minimal, structurally valid EPUB3 (`mimetype` + `container.xml` + an OPF
/// with `dc:title`/`dc:creator`/`dc:identifier`) — verified to parse via
/// `epub::EpubDocument::open` and yield non-empty `ExtractedMetadata`, so the
/// scanner creates a `Document` row for it.
pub fn sample_epub_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../features/fixtures/sample.epub")
}

/// A second EPUB fixture with title "Zeta Test Book" — used by scenarios that
/// need two distinct documents (e.g. `documents.sort`, `documents.merge`).
pub fn sample2_epub_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../features/fixtures/sample2.epub")
}

/// A third EPUB fixture ("Cover Sample Book") with an embedded cover image —
/// used by `documents.cover_display`. The OPF declares a manifest item with
/// `properties="cover-image"` so the scanner extracts and stores a cover.
pub fn sample_cover_epub_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../features/fixtures/sample_cover.epub")
}

/// A minimal valid PDF — used by `reading.pdf_viewer`. One page; passes MuPDF
/// structure checks so the scanner creates a `File`/`Content` row for it.
pub fn sample_pdf_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../features/fixtures/sample.pdf")
}

/// Serves `sample.epub` over a one-shot HTTP server on a random local port.
/// Returns the URL `http://127.0.0.1:{port}/sample.epub`.  The server shuts
/// down after handling the first request.  Used by `online_library.download_import`
/// to give `download_book` a real downloadable URL without hitting the internet.
pub async fn serve_epub_once() -> String {
    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    let epub_bytes = std::fs::read(sample_epub_path()).expect("read sample.epub fixture");
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind local HTTP fixture server");
    let addr = listener.local_addr().expect("local addr");
    let url = format!("http://{}/sample.epub", addr);

    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept connection");
        let mut buf = vec![0u8; 4096];
        let _ = stream.read(&mut buf).await; // discard request
        let header = format!(
            "HTTP/1.1 200 OK\r\n\
             Content-Type: application/epub+zip\r\n\
             Content-Length: {}\r\n\
             Connection: close\r\n\
             \r\n",
            epub_bytes.len()
        );
        stream
            .write_all(header.as_bytes())
            .await
            .expect("write header");
        stream.write_all(&epub_bytes).await.expect("write body");
    });

    url
}
