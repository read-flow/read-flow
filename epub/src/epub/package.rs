use std::collections::HashMap;

use quick_xml::Reader;
use quick_xml::events::Event;

use crate::content::resolve_href;
use crate::domain::metadata::DocumentMetadata;
use crate::domain::spine::SpineItem;
use crate::error::EpubError;
use crate::error::Result;

#[derive(Clone, Debug)]
pub struct ManifestItem {
    pub id: String,
    pub href: String,
    pub media_type: String,
    /// Space-separated properties value (e.g. `"nav"` for the EPUB3 nav document).
    pub properties: Option<String>,
}

#[derive(Debug)]
pub struct Package {
    pub metadata: DocumentMetadata,
    pub manifest: HashMap<String, ManifestItem>,
    pub spine: Vec<SpineItem>,
    /// Resolved zip path to the EPUB3 Navigation Document, if present.
    pub nav_href: Option<String>,
    /// Resolved zip path to the EPUB2 NCX document, if present.
    pub ncx_href: Option<String>,
    /// Resolved zip path to the cover image, if present.
    /// EPUB3: manifest item with `properties="cover-image"`.
    /// EPUB2: manifest item referenced by `<meta name="cover" content="item-id">`.
    pub cover_href: Option<String>,
}

impl Package {
    pub fn parse(xml: &[u8], opf_base: &str) -> Result<Self> {
        let mut reader = Reader::from_reader(xml);
        let mut buf = Vec::new();

        let mut metadata = DocumentMetadata::default();
        let mut manifest: HashMap<String, ManifestItem> = HashMap::new();
        let mut spine_refs: Vec<(String, bool)> = Vec::new();

        let mut in_metadata = false;
        let mut current_tag: Option<String> = None;
        // `toc` attribute on `<spine>` references the NCX item id (EPUB2)
        let mut ncx_idref: Option<String> = None;
        // EPUB2 `<meta name="cover" content="item-id">` cover manifest id
        let mut cover_meta_id: Option<String> = None;

        loop {
            match reader.read_event_into(&mut buf)? {
                Event::Start(ref e) => {
                    let name = e.name();
                    let local = local_name(name.as_ref());
                    match local {
                        b"metadata" => in_metadata = true,
                        b"title" | b"creator" | b"language" | b"publisher" | b"identifier"
                        | b"date"
                            if in_metadata =>
                        {
                            current_tag = Some(String::from_utf8_lossy(local).into_owned());
                        }
                        b"item" => {
                            if let Some(item) = parse_manifest_item(e, opf_base)? {
                                manifest.insert(item.id.clone(), item);
                            }
                        }
                        b"spine" => {
                            ncx_idref = parse_toc_attr(e)?;
                        }
                        b"itemref" => {
                            if let Some(spine_ref) = parse_spine_ref(e)? {
                                spine_refs.push(spine_ref);
                            }
                        }
                        _ => {}
                    }
                }
                Event::Empty(ref e) => {
                    let name = e.name();
                    let local = local_name(name.as_ref());
                    match local {
                        b"item" => {
                            if let Some(item) = parse_manifest_item(e, opf_base)? {
                                manifest.insert(item.id.clone(), item);
                            }
                        }
                        b"meta" if in_metadata => {
                            if let Some(id) = parse_cover_meta(e)? {
                                cover_meta_id = Some(id);
                            }
                        }
                        b"spine" => {
                            ncx_idref = parse_toc_attr(e)?;
                        }
                        b"itemref" => {
                            if let Some(spine_ref) = parse_spine_ref(e)? {
                                spine_refs.push(spine_ref);
                            }
                        }
                        _ => {}
                    }
                }
                Event::Text(ref e) if in_metadata && current_tag.is_some() => {
                    let text = e.xml_content().map_err(|err| {
                        EpubError::InvalidPackage(format!("text decode error: {err}"))
                    })?;
                    let text = text.trim().to_string();
                    if !text.is_empty() {
                        match current_tag.as_deref() {
                            Some("title") => metadata.title = Some(text),
                            Some("creator") => metadata.authors.push(text),
                            Some("language") => metadata.language = Some(text),
                            Some("publisher") => metadata.publisher = Some(text),
                            Some("identifier") => metadata.identifier = Some(text),
                            Some("date") => metadata.date = Some(text),
                            Some(tag) => {
                                tracing::debug!("unsupported EPUB metadata tag: {tag}");
                            }
                            None => {}
                        }
                    }
                }
                Event::End(ref e) => {
                    let name = e.name();
                    let local = local_name(name.as_ref());
                    if local == b"metadata" {
                        in_metadata = false;
                    }
                    current_tag = None;
                }
                Event::Eof => break,
                _ => {}
            }
            buf.clear();
        }

        // Resolve nav href: EPUB3 nav item has properties containing "nav"
        let nav_href = manifest
            .values()
            .find(|item| {
                item.properties
                    .as_deref()
                    .is_some_and(|p| p.split_whitespace().any(|w| w == "nav"))
            })
            .map(|item| item.href.clone());

        // Resolve NCX href: EPUB2 spine `toc` attribute references the NCX manifest id
        let ncx_href = ncx_idref
            .as_deref()
            .and_then(|id| manifest.get(id))
            .map(|item| item.href.clone());

        // Resolve cover href: EPUB3 cover-image property, or EPUB2 meta[name=cover]
        let cover_href = manifest
            .values()
            .find(|item| {
                item.properties
                    .as_deref()
                    .is_some_and(|p| p.split_whitespace().any(|w| w == "cover-image"))
            })
            .or_else(|| cover_meta_id.as_deref().and_then(|id| manifest.get(id)))
            .map(|item| item.href.clone());

        let spine = spine_refs
            .into_iter()
            .enumerate()
            .filter_map(|(index, (idref, linear))| {
                manifest.get(&idref).map(|item| SpineItem {
                    index,
                    id: item.id.clone(),
                    href: item.href.clone(),
                    linear,
                    label: None,
                })
            })
            .collect();

        Ok(Package {
            metadata,
            manifest,
            spine,
            nav_href,
            ncx_href,
            cover_href,
        })
    }
}

fn local_name(name: &[u8]) -> &[u8] {
    match name.iter().position(|&b| b == b':') {
        Some(pos) => &name[pos + 1..],
        None => name,
    }
}

fn parse_manifest_item(
    e: &quick_xml::events::BytesStart<'_>,
    opf_base: &str,
) -> Result<Option<ManifestItem>> {
    let mut id = None;
    let mut href = None;
    let mut media_type = None;
    let mut properties = None;

    for attr in e.attributes() {
        let attr =
            attr.map_err(|e| EpubError::InvalidPackage(format!("bad manifest attribute: {e}")))?;
        match attr.key.as_ref() {
            b"id" => id = Some(String::from_utf8_lossy(&attr.value).into_owned()),
            b"href" => {
                let raw = String::from_utf8_lossy(&attr.value).into_owned();
                href = Some(if opf_base.is_empty() {
                    raw
                } else {
                    resolve_href(opf_base, &raw)
                });
            }
            b"media-type" => media_type = Some(String::from_utf8_lossy(&attr.value).into_owned()),
            b"properties" => properties = Some(String::from_utf8_lossy(&attr.value).into_owned()),
            _ => {}
        }
    }

    match (id, href, media_type) {
        (Some(id), Some(href), Some(media_type)) => Ok(Some(ManifestItem {
            id,
            href,
            media_type,
            properties,
        })),
        _ => Ok(None),
    }
}

/// Extract the manifest item id from an EPUB2 `<meta name="cover" content="item-id">` element.
fn parse_cover_meta(e: &quick_xml::events::BytesStart<'_>) -> Result<Option<String>> {
    let mut name = None;
    let mut content = None;
    for attr in e.attributes() {
        let attr =
            attr.map_err(|e| EpubError::InvalidPackage(format!("bad meta attribute: {e}")))?;
        match attr.key.as_ref() {
            b"name" => name = Some(String::from_utf8_lossy(&attr.value).into_owned()),
            b"content" => content = Some(String::from_utf8_lossy(&attr.value).into_owned()),
            _ => {}
        }
    }
    Ok(match (name.as_deref(), content) {
        (Some("cover"), Some(id)) => Some(id),
        _ => None,
    })
}

/// Extract the `toc` attribute from a `<spine>` element (EPUB2 NCX reference).
fn parse_toc_attr(e: &quick_xml::events::BytesStart<'_>) -> Result<Option<String>> {
    for attr in e.attributes() {
        let attr =
            attr.map_err(|e| EpubError::InvalidPackage(format!("bad spine attribute: {e}")))?;
        if attr.key.as_ref() == b"toc" {
            return Ok(Some(String::from_utf8_lossy(&attr.value).into_owned()));
        }
    }
    Ok(None)
}

fn parse_spine_ref(e: &quick_xml::events::BytesStart<'_>) -> Result<Option<(String, bool)>> {
    let mut idref = None;
    let mut linear = true;

    for attr in e.attributes() {
        let attr =
            attr.map_err(|e| EpubError::InvalidPackage(format!("bad spine attribute: {e}")))?;
        match attr.key.as_ref() {
            b"idref" => idref = Some(String::from_utf8_lossy(&attr.value).into_owned()),
            b"linear" => linear = &*attr.value != b"no",
            _ => {}
        }
    }

    Ok(idref.map(|id| (id, linear)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPF: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>Test Book</dc:title>
    <dc:creator>Author One</dc:creator>
    <dc:creator>Author Two</dc:creator>
    <dc:language>en</dc:language>
    <dc:publisher>Test Publisher</dc:publisher>
    <dc:identifier>urn:isbn:1234567890</dc:identifier>
    <dc:date>2024-01-15</dc:date>
  </metadata>
  <manifest>
    <item id="c1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
    <item id="c2" href="chapter2.xhtml" media-type="application/xhtml+xml"/>
    <item id="css" href="style.css" media-type="text/css"/>
  </manifest>
  <spine>
    <itemref idref="c1"/>
    <itemref idref="c2" linear="no"/>
  </spine>
</package>"#;

    #[test]
    fn parses_metadata() {
        let pkg = Package::parse(OPF, "OEBPS").unwrap();
        assert_eq!(pkg.metadata.title.as_deref(), Some("Test Book"));
        assert_eq!(pkg.metadata.authors.len(), 2);
        assert_eq!(pkg.metadata.authors[0], "Author One");
        assert_eq!(pkg.metadata.authors[1], "Author Two");
        assert_eq!(pkg.metadata.language.as_deref(), Some("en"));
        assert_eq!(pkg.metadata.publisher.as_deref(), Some("Test Publisher"));
        assert_eq!(
            pkg.metadata.identifier.as_deref(),
            Some("urn:isbn:1234567890")
        );
        assert_eq!(pkg.metadata.date.as_deref(), Some("2024-01-15"));
    }

    #[test]
    fn parses_manifest_with_base_path() {
        let pkg = Package::parse(OPF, "OEBPS").unwrap();
        assert_eq!(pkg.manifest.len(), 3);
        let c1 = pkg.manifest.get("c1").unwrap();
        assert_eq!(c1.href, "OEBPS/chapter1.xhtml");
        assert_eq!(c1.media_type, "application/xhtml+xml");
    }

    #[test]
    fn parses_spine_order_and_linearity() {
        let pkg = Package::parse(OPF, "OEBPS").unwrap();
        assert_eq!(pkg.spine.len(), 2);

        assert_eq!(pkg.spine[0].index, 0);
        assert_eq!(pkg.spine[0].id, "c1");
        assert_eq!(pkg.spine[0].href, "OEBPS/chapter1.xhtml");
        assert!(pkg.spine[0].linear);

        assert_eq!(pkg.spine[1].index, 1);
        assert!(!pkg.spine[1].linear);
    }

    #[test]
    fn empty_base_path_leaves_href_unchanged() {
        let pkg = Package::parse(OPF, "").unwrap();
        let c1 = pkg.manifest.get("c1").unwrap();
        assert_eq!(c1.href, "chapter1.xhtml");
    }

    #[test]
    fn detects_epub3_nav_item() {
        let opf = br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>T</dc:title>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="c1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="c1"/></spine>
</package>"#;
        let pkg = Package::parse(opf, "OEBPS").unwrap();
        assert_eq!(pkg.nav_href.as_deref(), Some("OEBPS/nav.xhtml"));
        assert_eq!(pkg.ncx_href, None);
    }

    #[test]
    fn detects_epub2_ncx_item() {
        let opf = br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>T</dc:title>
  </metadata>
  <manifest>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    <item id="c1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine toc="ncx"><itemref idref="c1"/></spine>
</package>"#;
        let pkg = Package::parse(opf, "OEBPS").unwrap();
        assert_eq!(pkg.ncx_href.as_deref(), Some("OEBPS/toc.ncx"));
        assert_eq!(pkg.nav_href, None);
    }

    #[test]
    fn test_manifest_href_resolution() {
        // Test relative path resolution in manifest items using full package parsing
        let opf = br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata>
    <dc:title>Test Book</dc:title>
  </metadata>
  <manifest>
    <item id="chapter1" href="../Text/chapter1.xhtml" media-type="application/xhtml+xml"/>
    <item id="style1" href="../Styles/style.css" media-type="text/css"/>
  </manifest>
  <spine>
    <itemref idref="chapter1"/>
  </spine>
</package>"#;

        let pkg = Package::parse(opf, "OEBPS").unwrap();

        // Check that relative paths were resolved correctly
        let chapter1 = pkg.manifest.get("chapter1").unwrap();
        assert_eq!(chapter1.href, "Text/chapter1.xhtml"); // ../Text -> Text

        let style1 = pkg.manifest.get("style1").unwrap();
        assert_eq!(style1.href, "Styles/style.css"); // ../Styles -> Styles
    }

    #[test]
    fn test_manifest_href_absolute() {
        // Test absolute paths (starting with /) work correctly
        let opf = br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata>
    <dc:title>Test Book</dc:title>
  </metadata>
  <manifest>
    <item id="style1" href="/absolute/path.css" media-type="text/css"/>
  </manifest>
  <spine>
  </spine>
</package>"#;

        let pkg = Package::parse(opf, "OEBPS").unwrap();

        let style1 = pkg.manifest.get("style1").unwrap();
        // Absolute paths should have the leading / stripped
        assert_eq!(style1.href, "absolute/path.css");
    }

    #[test]
    fn test_manifest_href_no_base() {
        // Test when opf_base is empty
        let opf = br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata>
    <dc:title>Test Book</dc:title>
  </metadata>
  <manifest>
    <item id="chapter1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="chapter1"/>
  </spine>
</package>"#;

        let pkg = Package::parse(opf, "").unwrap();

        let chapter1 = pkg.manifest.get("chapter1").unwrap();
        // Should remain unchanged when no base is provided
        assert_eq!(chapter1.href, "chapter1.xhtml");
    }
}
