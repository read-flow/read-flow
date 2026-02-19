use std::collections::HashMap;

use quick_xml::Reader;
use quick_xml::events::Event;

use crate::content::base_dir;
use crate::content::resolve_href;

/// Parse an EPUB3 Navigation Document (`nav.xhtml`) and return a map from
/// resolved zip-path hrefs to human-readable labels.
///
/// Only entries inside a `<nav epub:type="toc">` element are collected.
/// If no typed nav element is found, all `<a href>` entries are used as a fallback.
pub fn parse_epub3_nav(xml: &[u8], nav_zip_href: &str) -> HashMap<String, String> {
    let base = base_dir(nav_zip_href);
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();

    // Two-pass: first try only the toc nav; if empty, fall back to all <a> elements.
    let result = extract_nav_links(&mut reader, &mut buf, base, true);
    if !result.is_empty() {
        return result;
    }

    // Reset and retry without the toc filter
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    extract_nav_links(&mut reader, &mut buf, base, false)
}

fn extract_nav_links(
    reader: &mut Reader<&[u8]>,
    buf: &mut Vec<u8>,
    base: &str,
    toc_only: bool,
) -> HashMap<String, String> {
    let mut map: HashMap<String, String> = HashMap::new();

    // Depth tracking for the toc nav element
    let mut in_toc_nav = !toc_only; // if not filtering, always "in" the nav
    let mut nav_depth: usize = 0; // nesting depth while inside the toc <nav>

    // Current <a> state
    let mut current_href: Option<String> = None;
    let mut current_text = String::new();
    let mut in_anchor = false;

    loop {
        buf.clear();
        match reader.read_event_into(buf) {
            Ok(Event::Start(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                match local {
                    b"nav" if toc_only => {
                        // Check for epub:type="toc" (or type="toc" with any prefix)
                        let is_toc = e.attributes().flatten().any(|a| {
                            let key = local_name(a.key.as_ref());
                            key == b"type"
                                && (a.value.as_ref() == b"toc"
                                    || a.value.as_ref().ends_with(b" toc")
                                    || a.value.as_ref().starts_with(b"toc "))
                        });
                        if is_toc {
                            in_toc_nav = true;
                            nav_depth = 1;
                        }
                    }
                    b"nav" if in_toc_nav && nav_depth > 0 => {
                        nav_depth += 1;
                    }
                    b"a" if in_toc_nav => {
                        let href = e.attributes().flatten().find_map(|a| {
                            if a.key.as_ref() == b"href" {
                                let raw = String::from_utf8_lossy(&a.value).into_owned();
                                // Strip fragment
                                let stripped = raw
                                    .split_once('#')
                                    .map(|(h, _)| h.to_string())
                                    .unwrap_or(raw);
                                if stripped.is_empty() {
                                    None
                                } else {
                                    Some(resolve_href(base, &stripped))
                                }
                            } else {
                                None
                            }
                        });
                        current_href = href;
                        current_text.clear();
                        in_anchor = true;
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                if local == b"a" && in_toc_nav {
                    // Self-closing <a/> — unusual but handle gracefully (no text to capture)
                }
            }
            Ok(Event::End(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                match local {
                    b"nav" if toc_only && in_toc_nav => {
                        nav_depth = nav_depth.saturating_sub(1);
                        if nav_depth == 0 {
                            in_toc_nav = false;
                        }
                    }
                    b"a" if in_anchor => {
                        if let Some(href) = current_href.take() {
                            let label = current_text.trim().to_string();
                            if !label.is_empty() {
                                // Use first label per href
                                map.entry(href).or_insert(label);
                            }
                        }
                        in_anchor = false;
                        current_text.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) if in_anchor => {
                if let Ok(t) = e.unescape() {
                    current_text.push_str(&t);
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }

    map
}

/// Parse an EPUB2 NCX document (`toc.ncx`) and return a map from
/// resolved zip-path hrefs to human-readable labels.
pub fn parse_epub2_ncx(xml: &[u8], ncx_zip_href: &str) -> HashMap<String, String> {
    let base = base_dir(ncx_zip_href);
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut map: HashMap<String, String> = HashMap::new();

    let mut in_navlabel_text = false;
    let mut pending_label = String::new();
    let mut capturing_label = false;

    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                match local {
                    b"navLabel" => capturing_label = true,
                    b"text" if capturing_label => {
                        in_navlabel_text = true;
                        pending_label.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                if local == b"content" {
                    // <content src="chapter.xhtml#anchor"/>
                    let src = e.attributes().flatten().find_map(|a| {
                        if a.key.as_ref() == b"src" {
                            let raw = String::from_utf8_lossy(&a.value).into_owned();
                            let stripped = raw
                                .split_once('#')
                                .map(|(h, _)| h.to_string())
                                .unwrap_or(raw);
                            if stripped.is_empty() {
                                None
                            } else {
                                Some(resolve_href(base, &stripped))
                            }
                        } else {
                            None
                        }
                    });
                    if let Some(href) = src {
                        let label = pending_label.trim().to_string();
                        if !label.is_empty() {
                            map.entry(href).or_insert(label);
                        }
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let name = e.name();
                let local = local_name(name.as_ref());
                match local {
                    b"text" if in_navlabel_text => {
                        in_navlabel_text = false;
                    }
                    b"navLabel" => {
                        capturing_label = false;
                    }
                    b"navPoint" => {
                        // Reset pending label when the navPoint closes
                        pending_label.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) if in_navlabel_text => {
                if let Ok(t) = e.unescape() {
                    pending_label.push_str(&t);
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }

    map
}

fn local_name(name: &[u8]) -> &[u8] {
    match name.iter().position(|&b| b == b':') {
        Some(pos) => &name[pos + 1..],
        None => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_epub3_nav() {
        let nav = br#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"
      xmlns:epub="http://www.idpf.org/2007/ops">
<body>
  <nav epub:type="toc">
    <ol>
      <li><a href="chapter1.xhtml">Chapter One</a></li>
      <li><a href="chapter2.xhtml">Chapter Two</a></li>
    </ol>
  </nav>
</body>
</html>"#;
        let map = parse_epub3_nav(nav, "OEBPS/nav.xhtml");
        assert_eq!(
            map.get("OEBPS/chapter1.xhtml").map(String::as_str),
            Some("Chapter One")
        );
        assert_eq!(
            map.get("OEBPS/chapter2.xhtml").map(String::as_str),
            Some("Chapter Two")
        );
    }

    #[test]
    fn parses_epub3_nav_strips_fragment() {
        let nav = br#"<html xmlns:epub="http://www.idpf.org/2007/ops">
<body><nav epub:type="toc"><ol>
  <li><a href="ch1.xhtml#start">First Chapter</a></li>
</ol></nav></body></html>"#;
        let map = parse_epub3_nav(nav, "OEBPS/nav.xhtml");
        assert!(map.contains_key("OEBPS/ch1.xhtml"));
        assert!(!map.contains_key("OEBPS/ch1.xhtml#start"));
    }

    #[test]
    fn parses_epub2_ncx() {
        let ncx = br#"<?xml version="1.0" encoding="UTF-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <navMap>
    <navPoint id="np1">
      <navLabel><text>Introduction</text></navLabel>
      <content src="intro.xhtml"/>
    </navPoint>
    <navPoint id="np2">
      <navLabel><text>Part One</text></navLabel>
      <content src="part1.xhtml#ch1"/>
    </navPoint>
  </navMap>
</ncx>"#;
        let map = parse_epub2_ncx(ncx, "OEBPS/toc.ncx");
        assert_eq!(
            map.get("OEBPS/intro.xhtml").map(String::as_str),
            Some("Introduction")
        );
        assert_eq!(
            map.get("OEBPS/part1.xhtml").map(String::as_str),
            Some("Part One")
        );
    }
}
