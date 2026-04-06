use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::RwLock;

use base64::Engine as _;
use zip::ZipArchive;

use crate::domain::document::Document;
use crate::domain::metadata::DocumentMetadata;
use crate::domain::nav::NavEntry;
use crate::domain::spine::SpineItem;
use crate::epub::container::Container;
use crate::epub::nav::parse_epub2_ncx;
use crate::epub::nav::parse_epub3_nav;
use crate::epub::package::Package;
use crate::error::EpubError;
use crate::error::Result;

pub struct EpubDocument {
    identifier: String,
    metadata: DocumentMetadata,
    spine: Vec<SpineItem>,
    nav: Vec<NavEntry>,
    archive: RwLock<ZipArchive<BufReader<File>>>,
}

impl EpubDocument {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let file = File::open(path.as_ref())?;
        let reader = BufReader::new(file);
        let mut archive = ZipArchive::new(reader)?;

        let container = Container::from_archive(&mut archive)?;

        let opf_base = container
            .rootfile_path
            .rfind('/')
            .map(|pos| &container.rootfile_path[..pos])
            .unwrap_or("")
            .to_string();

        let opf_contents = {
            let mut opf_file = archive
                .by_name(&container.rootfile_path)
                .map_err(|_| EpubError::MissingFile(container.rootfile_path.clone()))?;
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut opf_file, &mut buf)?;
            buf
        };

        let package = Package::parse(&opf_contents, &opf_base)?;

        let identifier = package
            .metadata
            .identifier
            .clone()
            .unwrap_or_else(|| container.rootfile_path.clone());

        // Build nav entries from the Navigation Document (EPUB3) or NCX (EPUB2).
        // EPUB3 nav takes priority; fall back to NCX if nav is absent.
        let nav_entries = read_nav_entries(&mut archive, &package);

        // Build a fragment-stripped href → label lookup (first match wins).
        let mut label_by_href: HashMap<String, String> = HashMap::new();
        for entry in &nav_entries {
            let base = entry
                .href
                .split_once('#')
                .map(|(b, _)| b)
                .unwrap_or(&entry.href);
            label_by_href
                .entry(base.to_string())
                .or_insert_with(|| entry.label.clone());
        }

        let spine = package
            .spine
            .into_iter()
            .map(|mut item| {
                item.label = label_by_href.get(&item.href).cloned();
                item
            })
            .collect();

        Ok(EpubDocument {
            identifier,
            metadata: package.metadata,
            spine,
            nav: nav_entries,
            archive: RwLock::new(archive),
        })
    }

    pub fn nav(&self) -> &[NavEntry] {
        &self.nav
    }
}

impl Document for EpubDocument {
    fn id(&self) -> &str {
        &self.identifier
    }

    fn metadata(&self) -> &DocumentMetadata {
        &self.metadata
    }

    fn spine(&self) -> &[SpineItem] {
        &self.spine
    }

    fn resolve_resource(&self, href: &str) -> Result<Vec<u8>> {
        // Data URLs carry their own payload — decode the base64 content directly.
        if let Some(rest) = href.strip_prefix("data:") {
            if let Some(encoded) = rest
                .split(';')
                .nth(1)
                .and_then(|s| s.strip_prefix("base64,"))
            {
                return base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .map_err(|_| EpubError::ResourceNotFound(href.to_string()));
            }
            return Err(EpubError::ResourceNotFound(href.to_string()));
        }

        let mut archive = self.archive.write().unwrap();
        let mut file = archive.by_name(href).map_err(|_| {
            tracing::info!("resource not found in EPUB archive: {href}");
            EpubError::ResourceNotFound(href.to_string())
        })?;
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut buf)?;
        Ok(buf)
    }
}

/// Try to read a nav / NCX document from the archive and return ordered nav entries.
/// Prefers EPUB3 nav; falls back to EPUB2 NCX.
fn read_nav_entries(archive: &mut ZipArchive<BufReader<File>>, package: &Package) -> Vec<NavEntry> {
    // Try EPUB3 nav first
    if let Some(nav_href) = &package.nav_href
        && let Ok(mut file) = archive.by_name(nav_href)
    {
        let mut buf = Vec::new();
        if std::io::Read::read_to_end(&mut file, &mut buf).is_ok() {
            let entries = parse_epub3_nav(&buf, nav_href);
            if !entries.is_empty() {
                return entries;
            }
        }
    }
    // Fall back to EPUB2 NCX
    if let Some(ncx_href) = &package.ncx_href
        && let Ok(mut file) = archive.by_name(ncx_href)
    {
        let mut buf = Vec::new();
        if std::io::Read::read_to_end(&mut file, &mut buf).is_ok() {
            return parse_epub2_ncx(&buf, ncx_href);
        }
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn create_test_epub(dir: &Path) -> std::path::PathBuf {
        let epub_path = dir.join("test.epub");
        let file = File::create(&epub_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);

        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);

        zip.start_file("mimetype", options).unwrap();
        zip.write_all(b"application/epub+zip").unwrap();

        zip.start_file("META-INF/container.xml", options).unwrap();
        zip.write_all(
            br#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#,
        )
        .unwrap();

        zip.start_file("OEBPS/content.opf", options).unwrap();
        zip.write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>Test EPUB</dc:title>
    <dc:creator>Test Author</dc:creator>
    <dc:identifier>test-id-123</dc:identifier>
  </metadata>
  <manifest>
    <item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="ch1"/>
  </spine>
</package>"#,
        )
        .unwrap();

        zip.start_file("OEBPS/chapter1.xhtml", options).unwrap();
        zip.write_all(b"<html><body><p>Hello, world!</p></body></html>")
            .unwrap();

        zip.finish().unwrap();
        epub_path
    }

    #[test]
    fn opens_and_reads_metadata() {
        let dir = std::env::temp_dir().join("epub_test_open");
        std::fs::create_dir_all(&dir).unwrap();
        let path = create_test_epub(&dir);

        let doc = EpubDocument::open(&path).unwrap();
        assert_eq!(doc.id(), "test-id-123");
        assert_eq!(doc.metadata().title.as_deref(), Some("Test EPUB"));
        assert_eq!(doc.metadata().authors, vec!["Test Author"]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reads_spine() {
        let dir = std::env::temp_dir().join("epub_test_spine");
        std::fs::create_dir_all(&dir).unwrap();
        let path = create_test_epub(&dir);

        let doc = EpubDocument::open(&path).unwrap();
        assert_eq!(doc.spine().len(), 1);
        assert_eq!(doc.spine()[0].href, "OEBPS/chapter1.xhtml");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn resolves_resource() {
        let dir = std::env::temp_dir().join("epub_test_resource");
        std::fs::create_dir_all(&dir).unwrap();
        let path = create_test_epub(&dir);

        let doc = EpubDocument::open(&path).unwrap();
        let content = doc.resolve_resource("OEBPS/chapter1.xhtml").unwrap();
        let text = String::from_utf8(content).unwrap();
        assert!(text.contains("Hello, world!"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn errors_on_missing_resource() {
        let dir = std::env::temp_dir().join("epub_test_missing");
        std::fs::create_dir_all(&dir).unwrap();
        let path = create_test_epub(&dir);

        let doc = EpubDocument::open(&path).unwrap();
        assert!(doc.resolve_resource("nonexistent.xhtml").is_err());

        std::fs::remove_dir_all(&dir).ok();
    }
}
