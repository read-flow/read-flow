//! EPUB chapter loading and a reusable libcosmic reading surface.
//!
//! Use [`reader::Reader`] for a paginated book with chapter navigation and a
//! restorable position, or [`page_view::PageView`] in a host that manages its
//! own reader state. Hosts supply persistence and handle view messages.

pub mod measure;
pub mod page_view;
pub mod pagination;
pub mod reader;
pub mod render;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use cosmic::widget;
use epub::ContentBlock;
use epub::Document as _;
use epub::EpubDocument;
use epub::StyleSheet;

#[derive(Clone, Debug)]
pub struct EpubChapter {
    pub label: String,
    /// Resolved zip path for this spine item (e.g. `OEBPS/Text/ch1.xhtml`).
    pub href: String,
    blocks: Arc<[ContentBlock]>,
    /// DOM node paths parallel to `blocks`, one per top-level block.
    /// Each path is a sequence of 0-based element-child indices starting from
    /// within `<html>` (body index first, then block index within body).
    block_paths: Vec<Vec<u32>>,
    /// Stable image handles keyed by the raw pointer of each image's data `Vec`.
    /// Created once so the handle ID is stable across frames; required because
    /// the wgpu renderer decodes images asynchronously keyed by handle ID —
    /// a fresh `Handle::from_bytes` call per frame would produce a new ID each
    /// time, preventing the decoded texture from ever being found in the cache.
    /// The pointer is valid for the lifetime of this chapter (data lives in `blocks`).
    image_handles: HashMap<usize, widget::image::Handle>,
    /// Raw XHTML source for debug display.
    pub raw_html: String,
}

impl EpubChapter {
    pub fn blocks(&self) -> &[ContentBlock] {
        &self.blocks
    }

    /// Locate a top-level target, including footnotes represented as blocks.
    pub fn block_for_fragment(&self, fragment: &str) -> Option<usize> {
        self.blocks.iter().position(|block| {
            matches!(block, ContentBlock::Anchor { id } | ContentBlock::Footnote { id, .. } if id == fragment)
        })
    }

    /// Build a locator for a top-level block in the reading order.
    pub fn locator(&self, chapter_idx: usize, block_idx: usize) -> epub::Locator {
        let node_path = self.block_paths.get(block_idx).cloned().unwrap_or_default();
        epub::Locator {
            spine_index: chapter_idx as u32,
            node_path,
            char_offset: 0,
        }
    }

    /// Find the flat block index in `chapter` whose stored node path best matches `target`.
    /// "Best match" means the stored path is the longest prefix of `target`, giving
    /// paragraph-level granularity for any CFI including text-node sub-offsets.
    pub fn block_index_for_path(&self, target: &[u32]) -> usize {
        if target.is_empty() {
            return 0;
        }
        self.block_paths
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.is_empty() && target.starts_with(p.as_slice()))
            .max_by_key(|(_, p)| p.len())
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    pub fn image_handles(&self) -> &HashMap<usize, widget::image::Handle> {
        &self.image_handles
    }

    fn new(
        label: String,
        href: String,
        blocks: Vec<ContentBlock>,
        block_paths: Vec<Vec<u32>>,
        raw_html: String,
    ) -> Self {
        let blocks: Arc<[ContentBlock]> = blocks.into();
        let mut image_handles = HashMap::new();
        collect_image_handles(&blocks, &mut image_handles);
        Self {
            label,
            href,
            blocks,
            block_paths,
            image_handles,
            raw_html,
        }
    }
}

/// Resolve embedded SVG image references in-place at chapter load time, so that
/// rendering doesn't need to read from the zip archive on every frame.
fn resolve_svg_blocks(blocks: &mut Vec<ContentBlock>, chapter_href: &str, doc: &EpubDocument) {
    for block in blocks {
        match block {
            ContentBlock::Svg { content, .. } => {
                *content =
                    epub::content::resolve_svg_images(content, chapter_href, &mut |img_path| {
                        match doc.resolve_resource(img_path) {
                            Ok(img_data) => {
                                let media_type = epub::content::guess_media_type(img_path);
                                Some((img_data, media_type))
                            }
                            Err(e) => {
                                tracing::info!(
                                    "SVG image resource not found in chapter \
                                 {chapter_href}: {img_path} ({e})"
                                );
                                None
                            }
                        }
                    });
            }
            ContentBlock::Figure { blocks, .. } => resolve_svg_blocks(blocks, chapter_href, doc),
            ContentBlock::BlockQuote { children } => {
                resolve_svg_blocks(children, chapter_href, doc)
            }
            ContentBlock::Footnote { blocks, .. } => resolve_svg_blocks(blocks, chapter_href, doc),
            _ => {}
        }
    }
}

/// Collect stable image handles keyed by data pointer, recursing into nested blocks.
fn collect_image_handles(blocks: &[ContentBlock], map: &mut HashMap<usize, widget::image::Handle>) {
    for block in blocks {
        match block {
            ContentBlock::Image { data, .. } if !data.is_empty() => {
                map.insert(
                    data.as_ptr() as usize,
                    widget::image::Handle::from_bytes(data.clone()),
                );
            }
            ContentBlock::Figure { blocks, .. } => collect_image_handles(blocks, map),
            ContentBlock::BlockQuote { children } => collect_image_handles(children, map),
            ContentBlock::Footnote { blocks, .. } => collect_image_handles(blocks, map),
            _ => {}
        }
    }
}

/// Load the linear chapters of an EPUB. Call from a blocking thread.
pub fn load_epub_chapters(
    path: &Path,
) -> Result<(String, Vec<EpubChapter>, EpubDocument), epub::EpubError> {
    let epub_doc = EpubDocument::open(path)?;

    let title = epub_doc.metadata().title.clone().unwrap_or_default();

    let spine = epub_doc.spine().to_vec();
    let mut chapters = Vec::with_capacity(spine.len());

    for (idx, item) in spine.iter().filter(|item| item.linear).enumerate() {
        let mut label = item
            .label
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| item.id.clone());
        if label.is_empty() {
            label = format!("Chapter {}", idx + 1);
        }

        let (blocks, block_paths, raw_html) = match epub_doc.resolve_resource(&item.href) {
            Ok(data) => parse_chapter(&epub_doc, &item.href, &data),
            Err(e) => {
                tracing::warn!("failed to resolve spine item {}: {e}", item.href);
                (Vec::new(), Vec::new(), String::new())
            }
        };

        chapters.push(EpubChapter::new(
            label,
            item.href.clone(),
            blocks,
            block_paths,
            raw_html,
        ));
    }

    Ok((title, chapters, epub_doc))
}

/// Load an archive chapter reached through a link outside the linear spine.
pub fn load_linked_chapter(doc: &EpubDocument, href: &str) -> Result<EpubChapter, epub::EpubError> {
    let data = doc.resolve_resource(href)?;
    let (blocks, block_paths, raw_html) = parse_chapter(doc, href, &data);
    Ok(EpubChapter::new(
        href.to_owned(),
        href.to_owned(),
        blocks,
        block_paths,
        raw_html,
    ))
}

fn parse_chapter(
    doc: &EpubDocument,
    href: &str,
    data: &[u8],
) -> (Vec<ContentBlock>, Vec<Vec<u32>>, String) {
    let raw_html = String::from_utf8_lossy(data).into_owned();
    let stylesheet = load_chapter_stylesheets(&raw_html, href, doc);
    let (mut blocks, block_paths) =
        epub::content::parse_xhtml_with_paths(data, href, &stylesheet, &mut |img_path| match doc
            .resolve_resource(img_path)
        {
            Ok(img_data) => Some((img_data, epub::content::guess_media_type(img_path))),
            Err(error) => {
                tracing::info!("image resource not found in chapter {href}: {img_path} ({error})");
                None
            }
        });
    resolve_svg_blocks(&mut blocks, href, doc);
    (blocks, block_paths, raw_html)
}

/// Extract `<link rel="stylesheet" href="...">` references and inline `<style>` blocks
/// from XHTML and load/parse them into a merged `StyleSheet`.
fn load_chapter_stylesheets(xhtml: &str, chapter_href: &str, doc: &EpubDocument) -> StyleSheet {
    let mut stylesheet = StyleSheet::empty();
    let base = epub::content::base_dir(chapter_href);

    // Extract linked external stylesheets: <link rel="stylesheet" href="..." />
    for segment in xhtml.split("<link") {
        let Some(end) = segment.find('>') else {
            continue;
        };
        let tag_content = &segment[..end];
        let lower = tag_content.to_ascii_lowercase();
        if !lower.contains("stylesheet") {
            continue;
        }
        let href = extract_attr_value(tag_content, "href");
        let Some(href) = href else { continue };

        let resolved = epub::content::resolve_href(base, href);
        match doc.resolve_resource(&resolved) {
            Ok(css_data) => {
                let css_text = String::from_utf8_lossy(&css_data);
                let sheet = epub::content::parse_css(&css_text);
                stylesheet.merge(sheet);
            }
            Err(e) => {
                tracing::debug!("failed to load stylesheet {resolved}: {e}");
            }
        }
    }

    // Extract inline <style> blocks from the <head>.
    let lower_xhtml = xhtml.to_ascii_lowercase();
    let mut search_start = 0;
    while let Some(tag_start) = lower_xhtml[search_start..].find("<style") {
        let abs_tag_start = search_start + tag_start;
        // Find the closing '>' of the opening tag (may have attributes like type="text/css")
        let Some(tag_end) = lower_xhtml[abs_tag_start..].find('>') else {
            break;
        };
        let content_start = abs_tag_start + tag_end + 1;
        let Some(close_pos) = lower_xhtml[content_start..].find("</style") else {
            break;
        };
        let css_text = &xhtml[content_start..content_start + close_pos];
        let sheet = epub::content::parse_css(css_text);
        stylesheet.merge(sheet);
        search_start = content_start + close_pos + "</style".len();
    }

    stylesheet
}

/// Extract the value of an HTML attribute from a tag fragment.
/// Handles both single and double-quoted values.
fn extract_attr_value<'a>(tag_content: &'a str, attr_name: &str) -> Option<&'a str> {
    // Find the attribute name (case-insensitive)
    let lower = tag_content.to_ascii_lowercase();
    let attr_pos = lower.find(attr_name)?;
    let rest = &tag_content[attr_pos + attr_name.len()..];
    // Skip whitespace and '='
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('=')?;
    let rest = rest.trim_start();
    // Extract quoted value
    if let Some(rest) = rest.strip_prefix('"') {
        let end = rest.find('"')?;
        Some(&rest[..end])
    } else if let Some(rest) = rest.strip_prefix('\'') {
        let end = rest.find('\'')?;
        Some(&rest[..end])
    } else {
        // Unquoted value (up to whitespace or >)
        let end = rest
            .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .unwrap_or(rest.len());
        if end > 0 { Some(&rest[..end]) } else { None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloned_chapter_keeps_its_image_handle() {
        let chapter = EpubChapter::new(
            "Chapter".into(),
            "chapter.xhtml".into(),
            vec![ContentBlock::Image {
                alt: "Illustration".into(),
                data: vec![1, 2, 3],
                media_type: "image/png".into(),
                natural_width: 0,
                natural_height: 0,
            }],
            vec![vec![1, 0]],
            String::new(),
        );
        let cloned = chapter.clone();
        let ContentBlock::Image { data, .. } = &cloned.blocks()[0] else {
            panic!("expected the chapter image");
        };
        let key = data.as_ptr() as usize;
        assert_eq!(cloned.image_handles()[&key], chapter.image_handles()[&key]);
    }
}
