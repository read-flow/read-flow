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
