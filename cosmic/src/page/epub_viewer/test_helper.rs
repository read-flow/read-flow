use std::io::Write as _;

use zip::write::SimpleFileOptions;

pub struct EpubBuilder {
    title: String,
    body_html: String,
    resources: Vec<(String, Vec<u8>, &'static str)>,
}

impl EpubBuilder {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body_html: String::new(),
            resources: Vec::new(),
        }
    }

    /// Set the chapter body as an inner HTML fragment.
    /// The builder wraps it in a minimal XHTML document automatically.
    pub fn body(mut self, html: impl Into<String>) -> Self {
        self.body_html = html.into();
        self
    }

    /// Add an optional resource at the given zip path relative to the EPUB root,
    /// e.g. `"OEBPS/images/cover.png"`.
    pub fn resource(
        mut self,
        zip_path: impl Into<String>,
        data: Vec<u8>,
        media_type: &'static str,
    ) -> Self {
        self.resources.push((zip_path.into(), data, media_type));
        self
    }

    /// Write the EPUB to a [`tempfile::NamedTempFile`] and return it.
    /// The caller **must** keep the returned handle alive while the EPUB is open.
    pub fn build(self) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().expect("create temp file");
        let mut zip = zip::ZipWriter::new(file);
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

        // --- mimetype (must be first, uncompressed) ---
        zip.start_file("mimetype", opts).unwrap();
        zip.write_all(b"application/epub+zip").unwrap();

        // --- META-INF/container.xml ---
        zip.start_file("META-INF/container.xml", opts).unwrap();
        zip.write_all(
            br#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#,
        )
        .unwrap();

        // --- OEBPS/content.opf ---
        zip.start_file("OEBPS/content.opf", opts).unwrap();
        let mut manifest_items = String::from(
            r#"    <item id="chapter" href="chapter.xhtml" media-type="application/xhtml+xml"/>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>"#,
        );
        for (i, (zip_path, _, media_type)) in self.resources.iter().enumerate() {
            let href = zip_path.strip_prefix("OEBPS/").unwrap_or(zip_path.as_str());
            manifest_items.push_str(&format!(
                "\n    <item id=\"res{i}\" href=\"{href}\" media-type=\"{media_type}\"/>"
            ));
        }
        let opf = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>{title}</dc:title>
    <dc:identifier>test-epub</dc:identifier>
  </metadata>
  <manifest>
{manifest_items}
  </manifest>
  <spine>
    <itemref idref="chapter"/>
  </spine>
</package>"#,
            title = self.title,
            manifest_items = manifest_items,
        );
        zip.write_all(opf.as_bytes()).unwrap();

        // --- OEBPS/nav.xhtml ---
        zip.start_file("OEBPS/nav.xhtml", opts).unwrap();
        let nav = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head><title>Table of Contents</title></head>
<body>
  <nav epub:type="toc">
    <ol>
      <li><a href="chapter.xhtml">{title}</a></li>
    </ol>
  </nav>
</body>
</html>"#,
            title = self.title,
        );
        zip.write_all(nav.as_bytes()).unwrap();

        // --- OEBPS/chapter.xhtml ---
        zip.start_file("OEBPS/chapter.xhtml", opts).unwrap();
        let chapter = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>{title}</title></head>
<body>
{body}
</body>
</html>"#,
            title = self.title,
            body = self.body_html,
        );
        zip.write_all(chapter.as_bytes()).unwrap();

        // --- additional resources ---
        for (zip_path, data, _) in self.resources {
            zip.start_file(&zip_path, opts).unwrap();
            zip.write_all(&data).unwrap();
        }

        zip.finish().expect("finish zip")
    }
}
