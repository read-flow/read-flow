use std::collections::HashMap;

use quick_xml::Reader;
use quick_xml::events::Event;

use crate::domain::metadata::DocumentMetadata;
use crate::domain::spine::SpineItem;
use crate::error::EpubError;
use crate::error::Result;

#[derive(Clone, Debug)]
pub struct ManifestItem {
    pub id: String,
    pub href: String,
    pub media_type: String,
}

#[derive(Debug)]
pub struct Package {
    pub metadata: DocumentMetadata,
    pub manifest: HashMap<String, ManifestItem>,
    pub spine: Vec<SpineItem>,
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

        loop {
            match reader.read_event_into(&mut buf)? {
                Event::Start(ref e) => {
                    let name = e.name();
                    let local = local_name(name.as_ref());
                    match local {
                        b"metadata" => in_metadata = true,
                        b"title" | b"creator" | b"language" | b"publisher" | b"identifier"
                            if in_metadata =>
                        {
                            current_tag = Some(String::from_utf8_lossy(local).into_owned());
                        }
                        b"item" => {
                            if let Some(item) = parse_manifest_item(e, opf_base)? {
                                manifest.insert(item.id.clone(), item);
                            }
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
                        b"itemref" => {
                            if let Some(spine_ref) = parse_spine_ref(e)? {
                                spine_refs.push(spine_ref);
                            }
                        }
                        _ => {}
                    }
                }
                Event::Text(ref e) if in_metadata && current_tag.is_some() => {
                    let text = e.unescape().map_err(|err| {
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
                            _ => {}
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

        let spine = spine_refs
            .into_iter()
            .enumerate()
            .filter_map(|(index, (idref, linear))| {
                manifest.get(&idref).map(|item| SpineItem {
                    index,
                    id: item.id.clone(),
                    href: item.href.clone(),
                    linear,
                })
            })
            .collect();

        Ok(Package {
            metadata,
            manifest,
            spine,
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
                    format!("{opf_base}/{raw}")
                });
            }
            b"media-type" => media_type = Some(String::from_utf8_lossy(&attr.value).into_owned()),
            _ => {}
        }
    }

    match (id, href, media_type) {
        (Some(id), Some(href), Some(media_type)) => Ok(Some(ManifestItem {
            id,
            href,
            media_type,
        })),
        _ => Ok(None),
    }
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
}
