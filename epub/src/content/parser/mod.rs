mod classify;
mod end_tag;
mod start_tag;
mod state;
mod util;

use html5ever::tokenizer::BufferQueue;
use html5ever::tokenizer::Tag;
use html5ever::tokenizer::TagKind;
use html5ever::tokenizer::Token;
use html5ever::tokenizer::TokenSink;
use html5ever::tokenizer::TokenSinkResult;
use html5ever::tokenizer::Tokenizer;
use html5ever::tokenizer::TokenizerOpts;
use state::ContentSink;
use state::in_preformatted;
use util::PendingImage;
// Re-export for stylesheet.rs (uses `super::parser::parse_css_declarations`)
pub(crate) use util::parse_css_declarations;

use super::block::BlockStyle;
use super::block::ContentBlock;
use super::block::InlineStyle;
use super::block::TextSpan;
use super::resolve::base_dir;
use super::resolve::resolve_href;
use super::stylesheet::StyleSheet;

impl TokenSink for ContentSink {
    type Handle = ();

    fn process_token(&self, token: Token, _line_number: u64) -> TokenSinkResult<Self::Handle> {
        let mut state = self.state.borrow_mut();

        match token {
            Token::TagToken(Tag {
                kind: TagKind::StartTag,
                ref name,
                ref attrs,
                self_closing,
                ..
            }) => {
                let tag_name = name.as_ref();
                start_tag::handle_start_tag(&mut state, tag_name, attrs, self_closing);
            }

            Token::TagToken(Tag {
                kind: TagKind::EndTag,
                ref name,
                ..
            }) => {
                let tag_name = name.as_ref();

                if state.skip_depth > 0 {
                    state.skip_depth -= 1;
                    return TokenSinkResult::Continue;
                }

                let entry = if state.stack.last().is_some_and(|e| e.tag == tag_name) {
                    let e = state.stack.pop().unwrap();
                    // Clear children-depth slot so sibling elements start their
                    // child counts from 0, not inherited from this element's subtree.
                    state.clear_child_count_for_closed_element();
                    e
                } else {
                    return TokenSinkResult::Continue;
                };

                end_tag::handle_end_tag(&mut state, tag_name, entry);
            }

            Token::CharacterTokens(text) => {
                if state.skip_depth > 0 {
                    return TokenSinkResult::Continue;
                }
                let in_pre = in_preformatted(&state.stack);
                let in_svg = state.stack.iter().any(|e| e.tag == "svg");
                if let Some(entry) = state.stack.last_mut() {
                    // Accumulate raw content for SVG elements (including text and tags)
                    if in_svg {
                        // Find the SVG entry to accumulate content
                        if let Some(svg_entry) = state.stack.iter_mut().find(|e| e.tag == "svg") {
                            svg_entry.svg_content.push_str(&text);
                        }
                    } else if in_pre {
                        entry.text.push_str(&text);
                    } else {
                        // Check whether the accumulated content (text buffer or last span)
                        // already ends with a space to avoid doubling.
                        let prev_ends_with_space = if !entry.text.is_empty() {
                            entry.text.ends_with(' ')
                        } else {
                            entry.spans.last().is_some_and(|s| s.text.ends_with(' '))
                        };
                        let normalized =
                            util::normalize_html_whitespace(&text, prev_ends_with_space);
                        entry.text.push_str(&normalized);
                    }
                }
            }

            Token::DoctypeToken(_) => {}
            Token::CommentToken(_) => {}
            Token::NullCharacterToken => {}
            Token::EOFToken => {}
            Token::ParseError(_) => {}
        }

        TokenSinkResult::Continue
    }
}

/// Parse XHTML content into structured content blocks.
///
/// - `xhtml`: raw XHTML bytes
/// - `chapter_href`: the zip path of this chapter (e.g. `"OEBPS/Text/ch1.xhtml"`),
///   used to resolve relative image paths
/// - `stylesheet`: CSS stylesheet for class-based styling
/// - `resolve_image`: callback that takes a resolved zip path and returns
///   `(data, media_type)` or `None` if the resource can't be found
pub fn parse_xhtml<F>(
    xhtml: &[u8],
    chapter_href: &str,
    stylesheet: &StyleSheet,
    resolve_image: &mut F,
) -> Vec<ContentBlock>
where
    F: FnMut(&str) -> Option<(Vec<u8>, String)>,
{
    let html_str = String::from_utf8_lossy(xhtml);

    let sink = ContentSink::new(chapter_href, stylesheet.clone());

    let tokenizer = Tokenizer::new(sink, TokenizerOpts::default());

    let buf = BufferQueue::default();
    buf.push_back(html5ever::tendril::StrTendril::from(html_str.as_ref()));
    let _ = tokenizer.feed(&buf);
    tokenizer.end();

    let (mut blocks, _paths, pending_images) = tokenizer.sink.into_blocks_and_pending();

    // Resolve pending images by walking the block tree
    if !pending_images.is_empty() {
        resolve_images(&mut blocks, &pending_images, resolve_image);
    }

    blocks
}

/// Like [`parse_xhtml`] but also returns a parallel vector of DOM node paths,
/// one per top-level block.
///
/// Each path is a slice of 0-based element-child indices starting from within
/// the `<html>` element (i.e. the first entry is the index of `<body>` among
/// `<html>`'s element children, typically `1`).  This encodes the same
/// information as the within-document part of an EPUB CFI after the `!`.
pub fn parse_xhtml_with_paths<F>(
    xhtml: &[u8],
    chapter_href: &str,
    stylesheet: &StyleSheet,
    resolve_image: &mut F,
) -> (Vec<ContentBlock>, Vec<Vec<u32>>)
where
    F: FnMut(&str) -> Option<(Vec<u8>, String)>,
{
    let html_str = String::from_utf8_lossy(xhtml);
    let sink = ContentSink::new(chapter_href, stylesheet.clone());
    let tokenizer = Tokenizer::new(sink, TokenizerOpts::default());
    let buf = BufferQueue::default();
    buf.push_back(html5ever::tendril::StrTendril::from(html_str.as_ref()));
    let _ = tokenizer.feed(&buf);
    tokenizer.end();

    let (mut blocks, mut paths, pending_images) = tokenizer.sink.into_blocks_and_pending();

    if !pending_images.is_empty() {
        resolve_images(&mut blocks, &pending_images, resolve_image);
    }

    // Ensure paths is the same length as blocks (pad with empty if necessary).
    paths.resize(blocks.len(), vec![]);

    (blocks, paths)
}

/// Resolve embedded image references in SVG content by converting xlink:href to data URIs.
/// This allows SVG with embedded images to render properly when the SVG renderer doesn't
/// have access to the EPUB's resource resolution system.
pub fn resolve_svg_images<F>(svg_content: &str, chapter_href: &str, resolve_image: &mut F) -> String
where
    F: FnMut(&str) -> Option<(Vec<u8>, String)>,
{
    use regex::Regex;

    // Simple regex-based approach to find and replace xlink:href attributes
    let re = Regex::new(r#"<image([^>]*?)\s+xlink:href="([^"]*)"([^>]*?)>"#).unwrap();

    let result = re.replace_all(svg_content, |caps: &regex::Captures| {
        let before_href = &caps[1];
        let href = &caps[2];
        let after_href = &caps[3];

        // Resolve the href against the chapter's directory (handles all relative
        // forms: "file.jpg", "./file.jpg", "../dir/file.jpg", data: URIs, etc.)
        let resolved_path = resolve_href(base_dir(chapter_href), href);

        // Resolve the image reference
        if let Some((image_data, media_type)) = resolve_image(&resolved_path) {
            // Convert to data URI
            use base64::Engine;
            let base64_data = base64::engine::general_purpose::STANDARD.encode(&image_data);
            let data_uri = format!("data:{};base64,{}", media_type, base64_data);

            format!(
                r#"<image{} xlink:href="{}"{}>"#,
                before_href, data_uri, after_href
            )
        } else {
            // Keep original if resolution fails
            format!(
                r#"<image{} xlink:href="{}"{}>"#,
                before_href, href, after_href
            )
        }
    });

    result.to_string()
}

/// Decode the pixel dimensions of a raster image from raw bytes without full decoding.
/// Returns `(0, 0)` if the format is unsupported or the data is corrupt.
fn decode_image_dimensions(data: &[u8]) -> (u32, u32) {
    image::ImageReader::new(std::io::Cursor::new(data))
        .with_guessed_format()
        .ok()
        .and_then(|r| r.into_dimensions().ok())
        .unwrap_or((0, 0))
}

/// Search an SVG source string for a `viewBox` attribute and return the width/height ratio.
fn parse_viewbox_aspect_ratio(svg: &str) -> Option<f32> {
    let idx = svg.find("viewBox")?;
    let rest = svg[idx + "viewBox".len()..]
        .trim_start()
        .strip_prefix('=')?;
    let rest = rest.trim_start();
    let vb = if let Some(s) = rest.strip_prefix('"') {
        s.split('"').next()?
    } else if let Some(s) = rest.strip_prefix('\'') {
        s.split('\'').next()?
    } else {
        return None;
    };
    util::parse_viewbox_str(vb)
}

/// Walk the block tree and resolve placeholder Image blocks.
fn resolve_images<F>(
    blocks: &mut [ContentBlock],
    pending: &[(usize, PendingImage)],
    resolve_image: &mut F,
) where
    F: FnMut(&str) -> Option<(Vec<u8>, String)>,
{
    for block in blocks.iter_mut() {
        match block {
            ContentBlock::Image {
                alt,
                data,
                media_type,
                natural_width,
                natural_height,
            } if data.is_empty() => {
                // This is a placeholder — find matching pending image
                if let Some((_, img)) = pending.iter().find(|(_, img)| img.alt == *alt) {
                    if let Some((resolved_data, resolved_mt)) = resolve_image(&img.resolved_path) {
                        (*natural_width, *natural_height) = decode_image_dimensions(&resolved_data);
                        *data = resolved_data;
                        *media_type = resolved_mt;
                    } else {
                        // Replace with alt-text paragraph fallback
                        if !alt.is_empty() {
                            let alt_text = format!("[{}]", alt);
                            *block = ContentBlock::Paragraph {
                                spans: vec![TextSpan {
                                    text: alt_text.clone(),
                                    style: InlineStyle::default(),
                                    link: None,
                                    color: None,
                                    font_size_em: None,
                                }],
                                text: alt_text,
                                style: BlockStyle::default(),
                            };
                        } else {
                            *block = ContentBlock::HorizontalRule; // will be filtered
                        }
                    }
                }
            }
            ContentBlock::Svg {
                alt,
                content,
                aspect_ratio,
                ..
            } if content.is_empty() => {
                // This is a placeholder SVG from img tag - find matching pending image
                if let Some((_, img)) = pending.iter().find(|(_, img)| img.alt == *alt) {
                    if let Some((resolved_data, resolved_mt)) = resolve_image(&img.resolved_path) {
                        if resolved_mt == "image/svg+xml" {
                            let svg_str = String::from_utf8_lossy(&resolved_data).into_owned();
                            *aspect_ratio = parse_viewbox_aspect_ratio(&svg_str);
                            *content = svg_str;
                        } else {
                            // Wrong media type - replace with alt-text fallback
                            if !alt.is_empty() {
                                let alt_text = format!("[{}]", alt);
                                *block = ContentBlock::Paragraph {
                                    spans: vec![TextSpan {
                                        text: alt_text.clone(),
                                        style: InlineStyle::default(),
                                        link: None,
                                        color: None,
                                        font_size_em: None,
                                    }],
                                    text: alt_text,
                                    style: BlockStyle::default(),
                                };
                            } else {
                                *block = ContentBlock::HorizontalRule; // will be filtered
                            }
                        }
                    } else {
                        // Replace with alt-text paragraph fallback
                        if !alt.is_empty() {
                            let alt_text = format!("[{}]", alt);
                            *block = ContentBlock::Paragraph {
                                spans: vec![TextSpan {
                                    text: alt_text.clone(),
                                    style: InlineStyle::default(),
                                    link: None,
                                    color: None,
                                    font_size_em: None,
                                }],
                                text: alt_text,
                                style: BlockStyle::default(),
                            };
                        } else {
                            *block = ContentBlock::HorizontalRule; // will be filtered
                        }
                    }
                }
            }
            ContentBlock::BlockQuote { children } => {
                resolve_images(children, pending, resolve_image);
            }
            ContentBlock::Figure { blocks, .. } => {
                resolve_images(blocks, pending, resolve_image);
            }
            ContentBlock::Footnote { blocks, .. } => {
                resolve_images(blocks, pending, resolve_image);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::util::normalize_html_whitespace;
    use super::util::parse_css_color;
    use super::util::parse_css_length_as_em;
    use super::util::parse_inline_style;
    use super::*;
    use crate::TextAlign;

    fn parse(html: &str) -> Vec<ContentBlock> {
        parse_xhtml(
            html.as_bytes(),
            "OEBPS/Text/ch1.xhtml",
            &StyleSheet::empty(),
            &mut |_| None,
        )
    }

    #[test]
    fn parses_paragraph() {
        let blocks = parse("<html><body><p>Hello world</p></body></html>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { text, spans, .. } => {
                assert_eq!(text, "Hello world");
                assert_eq!(spans.len(), 1);
                assert_eq!(spans[0].text, "Hello world");
                assert_eq!(spans[0].style, InlineStyle::default());
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn parses_headings() {
        let blocks = parse("<h1>Title</h1><h2>Subtitle</h2><h3>Section</h3>");
        assert_eq!(blocks.len(), 3);
        match &blocks[0] {
            ContentBlock::Heading { level: 1, text, .. } => assert_eq!(text, "Title"),
            other => panic!("expected Heading h1, got {other:?}"),
        }
        match &blocks[1] {
            ContentBlock::Heading { level: 2, text, .. } => assert_eq!(text, "Subtitle"),
            other => panic!("expected Heading h2, got {other:?}"),
        }
        match &blocks[2] {
            ContentBlock::Heading { level: 3, text, .. } => assert_eq!(text, "Section"),
            other => panic!("expected Heading h3, got {other:?}"),
        }
    }

    #[test]
    fn skips_empty_paragraphs() {
        let blocks = parse("<p></p><p>  </p><p>content</p>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { text, .. } => assert_eq!(text, "content"),
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn condense_white_space() {
        let blocks = parse("<p></p><p>  </p><p>content</p>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { text, .. } => assert_eq!(text, "content"),
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn parses_unordered_list() {
        let blocks = parse("<ul><li>one</li><li>two</li><li>three</li></ul>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::UnorderedList { items } => {
                assert_eq!(items.len(), 3);
                assert_eq!(items[0].text, "one");
                assert_eq!(items[1].text, "two");
                assert_eq!(items[2].text, "three");
            }
            other => panic!("expected UnorderedList, got {other:?}"),
        }
    }

    #[test]
    fn parses_ordered_list() {
        let blocks = parse("<ol start=\"5\"><li>a</li><li>b</li></ol>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::OrderedList { start, items } => {
                assert_eq!(*start, 5);
                assert_eq!(items.len(), 2);
                assert_eq!(items[0].text, "a");
            }
            other => panic!("expected OrderedList, got {other:?}"),
        }
    }

    #[test]
    fn parses_blockquote() {
        let blocks = parse("<blockquote><p>Quoted text</p></blockquote>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::BlockQuote { children } => {
                assert_eq!(children.len(), 1);
                match &children[0] {
                    ContentBlock::Paragraph { text, .. } => assert_eq!(text, "Quoted text"),
                    other => panic!("expected Paragraph inside blockquote, got {other:?}"),
                }
            }
            other => panic!("expected BlockQuote, got {other:?}"),
        }
    }

    #[test]
    fn parses_preformatted() {
        let blocks = parse("<pre>  code here\n  indented</pre>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Preformatted { text, .. } => {
                assert!(
                    text.starts_with("  code here"),
                    "leading whitespace must be preserved: {text:?}"
                );
                assert!(text.contains("\n  indented"));
            }
            other => panic!("expected Preformatted, got {other:?}"),
        }
    }

    #[test]
    fn parses_horizontal_rule() {
        let blocks = parse("<p>before</p><hr/><p>after</p>");
        assert_eq!(blocks.len(), 3);
        assert!(matches!(blocks[1], ContentBlock::HorizontalRule));
    }

    #[test]
    fn inline_bold_produces_styled_spans() {
        let blocks = parse("<p>This is <strong>bold</strong> text</p>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { text, spans, .. } => {
                assert_eq!(text, "This is bold text");
                assert_eq!(spans.len(), 3);
                assert_eq!(spans[0].text, "This is ");
                assert!(!spans[0].style.bold);
                assert_eq!(spans[1].text, "bold");
                assert!(spans[1].style.bold);
                assert_eq!(spans[2].text, " text");
                assert!(!spans[2].style.bold);
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn inline_italic_produces_styled_spans() {
        let blocks = parse("<p>This is <em>italic</em> text</p>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { text, spans, .. } => {
                assert_eq!(text, "This is italic text");
                assert!(spans[1].style.italic);
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn nested_bold_italic_produces_combined_style() {
        let blocks = parse("<p><strong><em>bold italic</em></strong></p>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { text, spans, .. } => {
                assert_eq!(text, "bold italic");
                assert_eq!(spans.len(), 1);
                assert!(spans[0].style.bold);
                assert!(spans[0].style.italic);
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn underline_and_strikethrough() {
        let blocks = parse("<p><u>underlined</u> and <del>deleted</del></p>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { spans, .. } => {
                assert!(spans[0].style.underline);
                assert!(spans[2].style.strikethrough);
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn inline_styling_in_list_items() {
        let blocks = parse("<ul><li>normal <strong>bold</strong> text</li></ul>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::UnorderedList { items } => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].text, "normal bold text");
                assert_eq!(items[0].spans.len(), 3);
                assert!(items[0].spans[1].style.bold);
            }
            other => panic!("expected UnorderedList, got {other:?}"),
        }
    }

    #[test]
    fn transparent_containers_promote_children() {
        let blocks = parse("<div><p>inside div</p></div><section><p>inside section</p></section>");
        assert_eq!(blocks.len(), 2);
        match &blocks[0] {
            ContentBlock::Paragraph { text, .. } => assert_eq!(text, "inside div"),
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn image_with_resolver() {
        let png_data = vec![0x89, 0x50, 0x4E, 0x47];
        let data_clone = png_data.clone();
        let blocks = parse_xhtml(
            b"<img src=\"../Images/cover.png\" alt=\"Cover\"/>",
            "OEBPS/Text/ch1.xhtml",
            &StyleSheet::empty(),
            &mut move |path| {
                if path == "OEBPS/Images/cover.png" {
                    Some((data_clone.clone(), "image/png".to_string()))
                } else {
                    None
                }
            },
        );
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Image {
                alt,
                data,
                media_type,
                ..
            } => {
                assert_eq!(alt, "Cover");
                assert_eq!(data, &png_data);
                assert_eq!(media_type, "image/png");
            }
            other => panic!("expected Image, got {other:?}"),
        }
    }

    #[test]
    fn image_fallback_to_alt_text() {
        let blocks = parse("<img src=\"missing.png\" alt=\"A picture\"/>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { text, .. } => assert_eq!(text, "[A picture]"),
            other => panic!("expected fallback Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn skips_head_and_script() {
        let blocks = parse(
            "<html><head><title>Skip</title><style>body{}</style></head><body><p>Keep</p><script>alert(1)</script></body></html>",
        );
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { text, .. } => assert_eq!(text, "Keep"),
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn head_with_void_elements_does_not_swallow_body() {
        let blocks = parse(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml">
<head>
  <meta charset="UTF-8"/>
  <title>Test</title>
  <link rel="stylesheet" href="style.css"/>
</head>
<body>
  <h1>Chapter One</h1>
  <p>Body text here.</p>
</body>
</html>"#,
        );
        assert!(
            blocks.len() >= 2,
            "expected at least 2 blocks, got {}: {blocks:?}",
            blocks.len()
        );
        match &blocks[0] {
            ContentBlock::Heading { level: 1, text, .. } => assert_eq!(text, "Chapter One"),
            other => panic!("expected Heading h1, got {other:?}"),
        }
        match &blocks[1] {
            ContentBlock::Paragraph { text, .. } => assert_eq!(text, "Body text here."),
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn br_inserts_newline() {
        let blocks = parse("<p>line one<br/>line two</p>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { text, .. } => {
                assert!(text.contains("line one"));
                assert!(text.contains("line two"));
                assert!(text.contains('\n'));
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn link_text_not_duplicated() {
        // Regression: <a> was transparent — its text must appear exactly once.
        let blocks = parse("<p>Click <a href=\"ch2.xhtml\">here</a> for more.</p>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { text, spans, .. } => {
                assert_eq!(text, "Click here for more.");
                let here_count = spans.iter().filter(|s| s.text.contains("here")).count();
                assert_eq!(here_count, 1, "link text duplicated in spans: {spans:?}");
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn link_href_captured_on_span() {
        let blocks = parse("<p>Go to <a href=\"ch2.xhtml\">chapter two</a>.</p>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { spans, .. } => {
                let linked: Vec<_> = spans.iter().filter(|s| s.link.is_some()).collect();
                assert_eq!(linked.len(), 1);
                assert_eq!(linked[0].text, "chapter two");
                assert_eq!(linked[0].link.as_deref(), Some("ch2.xhtml"));
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn link_with_styled_content_not_duplicated() {
        // Link containing bold text must not duplicate either the bold span or the plain text.
        let blocks = parse("<p>See <a href=\"ch2.xhtml\"><strong>bold link</strong></a> here.</p>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { text, spans, .. } => {
                assert_eq!(text, "See bold link here.");
                let bold_link_count = spans
                    .iter()
                    .filter(|s| s.text.contains("bold link"))
                    .count();
                assert_eq!(bold_link_count, 1, "bold link text duplicated: {spans:?}");
                // The bold span must also carry the link
                let bold_span = spans.iter().find(|s| s.text.contains("bold link")).unwrap();
                assert!(bold_span.style.bold);
                assert_eq!(bold_span.link.as_deref(), Some("ch2.xhtml"));
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn plain_spans_have_no_link() {
        let blocks = parse("<p>No links here.</p>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { spans, .. } => {
                assert!(spans.iter().all(|s| s.link.is_none()));
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn parses_simple_table() {
        let blocks = parse(
            "<table>\
               <thead><tr><th>Name</th><th>Value</th></tr></thead>\
               <tbody><tr><td>Foo</td><td>42</td></tr></tbody>\
             </table>",
        );
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Table { rows } => {
                assert_eq!(rows.len(), 2);
                assert_eq!(rows[0].len(), 2);
                assert!(rows[0][0].is_header);
                assert_eq!(rows[0][0].text, "Name");
                assert!(rows[0][1].is_header);
                assert_eq!(rows[0][1].text, "Value");
                assert!(!rows[1][0].is_header);
                assert_eq!(rows[1][0].text, "Foo");
                assert!(!rows[1][1].is_header);
                assert_eq!(rows[1][1].text, "42");
            }
            other => panic!("expected Table, got {other:?}"),
        }
    }

    #[test]
    fn parses_table_without_thead() {
        let blocks =
            parse("<table><tr><td>A</td><td>B</td></tr><tr><td>C</td><td>D</td></tr></table>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Table { rows } => {
                assert_eq!(rows.len(), 2);
                assert_eq!(rows[0][0].text, "A");
                assert_eq!(rows[0][1].text, "B");
                assert_eq!(rows[1][0].text, "C");
            }
            other => panic!("expected Table, got {other:?}"),
        }
    }

    #[test]
    fn table_cell_with_inline_styles() {
        let blocks = parse("<table><tr><td>plain</td><td><strong>bold</strong></td></tr></table>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Table { rows } => {
                assert_eq!(rows[0].len(), 2);
                assert_eq!(rows[0][0].text, "plain");
                assert_eq!(rows[0][1].text, "bold");
                assert_eq!(rows[0][1].spans.len(), 1);
                assert!(rows[0][1].spans[0].style.bold);
            }
            other => panic!("expected Table, got {other:?}"),
        }
    }

    #[test]
    fn table_cell_with_paragraph_child() {
        // <p> inside <td> — content should be extracted from children
        let blocks = parse("<table><tr><td><p>Cell content</p></td></tr></table>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Table { rows } => {
                assert_eq!(rows[0].len(), 1);
                assert_eq!(rows[0][0].text, "Cell content");
            }
            other => panic!("expected Table, got {other:?}"),
        }
    }

    #[test]
    fn epub3_aside_footnote() {
        let blocks = parse(
            r##"<p>Text<sup><a href="#fn1">1</a></sup>.</p>
               <aside epub:type="footnote" id="fn1"><p>Footnote text.</p></aside>"##,
        );
        // Should have a Paragraph and a Footnote
        assert_eq!(blocks.len(), 2);
        match &blocks[1] {
            ContentBlock::Footnote { id, blocks } => {
                assert_eq!(id, "fn1");
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ContentBlock::Paragraph { text, .. } => assert_eq!(text, "Footnote text."),
                    other => panic!("expected Paragraph inside Footnote, got {other:?}"),
                }
            }
            other => panic!("expected Footnote, got {other:?}"),
        }
    }

    #[test]
    fn pandoc_footnotes_section() {
        // Pandoc-style: <section class="footnotes"><ol><li id="fn1">…</li></ol></section>
        let blocks = parse(
            r##"<p>Text<a href="#fn1"><sup>1</sup></a>.</p>
               <section class="footnotes" role="doc-endnotes">
                 <ol>
                   <li id="fn1"><p>First note.</p></li>
                   <li id="fn2"><p>Second note.</p></li>
                 </ol>
               </section>"##,
        );
        let footnotes: Vec<_> = blocks
            .iter()
            .filter(|b| matches!(b, ContentBlock::Footnote { .. }))
            .collect();
        assert_eq!(footnotes.len(), 2);
        match &footnotes[0] {
            ContentBlock::Footnote { id, blocks } => {
                assert_eq!(id, "fn1");
                match &blocks[0] {
                    ContentBlock::Paragraph { text, .. } => assert_eq!(text, "First note."),
                    other => panic!("expected Paragraph, got {other:?}"),
                }
            }
            other => panic!("expected Footnote, got {other:?}"),
        }
        match &footnotes[1] {
            ContentBlock::Footnote { id, .. } => assert_eq!(id, "fn2"),
            other => panic!("expected Footnote, got {other:?}"),
        }
    }

    #[test]
    fn aside_without_footnote_type_is_transparent() {
        // A plain <aside> with no epub:type should promote its children normally
        let blocks = parse("<aside><p>Side content</p></aside>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { text, .. } => assert_eq!(text, "Side content"),
            other => panic!("expected transparent aside → Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn fragment_only_link_does_not_create_footnote() {
        // A pure #anchor link in the text body should still render as a linked span
        let blocks = parse(r##"<p>See <a href="#fn1">note 1</a>.</p>"##);
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { spans, .. } => {
                let linked = spans.iter().find(|s| s.link.is_some()).unwrap();
                assert_eq!(linked.link.as_deref(), Some("#fn1"));
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn inline_anchor_id_emits_anchor_before_paragraph() {
        // Pandoc-style: <a id="fnref1" href="#fn1"> — the id should become an
        // Anchor block before the containing paragraph so that the footnote's
        // back-reference link (href="#fnref1") can navigate to the call site.
        let blocks = parse(r##"<p>Text <a id="fnref1" href="#fn1">1</a> continues.</p>"##);
        // Expect: Anchor { id: "fnref1" }, Paragraph { ... }
        assert_eq!(
            blocks.len(),
            2,
            "expected Anchor + Paragraph, got {blocks:?}"
        );
        match &blocks[0] {
            ContentBlock::Anchor { id } => assert_eq!(id, "fnref1"),
            other => panic!("expected Anchor, got {other:?}"),
        }
        match &blocks[1] {
            ContentBlock::Paragraph { spans, .. } => {
                let linked = spans.iter().find(|s| s.link.is_some()).unwrap();
                assert_eq!(linked.link.as_deref(), Some("#fn1"));
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn multiple_inline_anchor_ids_each_get_anchor_before_paragraph() {
        // Two references to the same footnote, each with a distinct id.
        let blocks = parse(
            r##"<p>First <a id="fnref1" href="#fn1">1</a>.</p>
                <p>Second <a id="fnref1-1" href="#fn1">1</a>.</p>"##,
        );
        // Expect: Anchor fnref1, Paragraph, Anchor fnref1-1, Paragraph
        assert_eq!(blocks.len(), 4, "got {blocks:?}");
        assert!(matches!(&blocks[0], ContentBlock::Anchor { id } if id == "fnref1"));
        assert!(matches!(&blocks[1], ContentBlock::Paragraph { .. }));
        assert!(matches!(&blocks[2], ContentBlock::Anchor { id } if id == "fnref1-1"));
        assert!(matches!(&blocks[3], ContentBlock::Paragraph { .. }));
    }

    #[test]
    fn self_closing_anchor_does_not_swallow_subsequent_siblings() {
        // XHTML self-closing named anchors like <a id="link1"/> must not leave
        // an unclosed entry on the stack. If they do, every subsequent sibling
        // element is silently consumed and lost (including </body>).
        let blocks = parse(r##"<p><a id="link1"/>First paragraph.</p><p>Second paragraph.</p>"##);
        let paragraphs: Vec<_> = blocks
            .iter()
            .filter(|b| matches!(b, ContentBlock::Paragraph { .. }))
            .collect();
        assert_eq!(
            paragraphs.len(),
            2,
            "both paragraphs must be present: {blocks:?}"
        );
    }

    // --- normalize_html_whitespace unit tests ---

    #[test]
    fn normalize_collapses_spaces() {
        assert_eq!(normalize_html_whitespace("a  b   c", false), "a b c");
    }

    #[test]
    fn normalize_collapses_tabs_and_newlines() {
        assert_eq!(normalize_html_whitespace("a\t\nb", false), "a b");
    }

    #[test]
    fn normalize_suppresses_leading_space_when_prev_ends_with_space() {
        assert_eq!(normalize_html_whitespace(" world", true), "world");
    }

    #[test]
    fn normalize_keeps_leading_space_when_prev_does_not_end_with_space() {
        assert_eq!(normalize_html_whitespace(" world", false), " world");
    }

    // --- Integration-level whitespace tests ---

    #[test]
    fn inline_whitespace_collapsed_in_paragraph() {
        // Multi-line source like EPUB XML often has newlines and extra spaces
        let blocks = parse("<p>Hello\n  world</p>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { text, .. } => assert_eq!(text, "Hello world"),
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn whitespace_preserved_inside_pre() {
        let blocks = parse("<pre>line one\n  indented\nline three</pre>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Preformatted { text, .. } => {
                assert!(text.contains('\n'), "newlines must be kept in <pre>");
                assert!(
                    text.contains("  indented"),
                    "indentation must be kept in <pre>"
                );
            }
            other => panic!("expected Preformatted, got {other:?}"),
        }
    }

    #[test]
    fn pre_with_css_class_colors_applied() {
        // Pandoc syntax-highlighting: `code span.kw` rules should apply color to
        // `<span class="kw">` inside `<pre><code>`.
        use crate::content::stylesheet::parse_css;
        let css = "code span.kw { color: #007020; font-weight: bold; }
                   code span.co { color: #60a0b0; font-style: italic; }";
        let stylesheet = parse_css(css);
        let html = r#"<pre class="sourceCode"><code class="sourceCode rust"><span class="kw">fn</span> main() <span class="co">// comment</span></code></pre>"#;
        let blocks = parse_xhtml(
            html.as_bytes(),
            "OEBPS/Text/ch1.xhtml",
            &stylesheet,
            &mut |_| None,
        );
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Preformatted { spans, .. } => {
                let kw = spans.iter().find(|s| s.text == "fn").expect("kw span");
                assert_eq!(kw.color, Some([0x00, 0x70, 0x20]), "keyword color");
                assert!(kw.style.bold, "keyword bold");
                let co = spans
                    .iter()
                    .find(|s| s.text == "// comment")
                    .expect("co span");
                assert_eq!(co.color, Some([0x60, 0xa0, 0xb0]), "comment color");
                assert!(co.style.italic, "comment italic");
            }
            other => panic!("expected Preformatted, got {other:?}"),
        }
    }

    #[test]
    fn pre_with_styled_spans_preserves_whitespace() {
        // <span> is a TRANSPARENT_TAG and goes through block-level handling.
        // Whitespace inside spans nested in <pre> must NOT be trimmed.
        let blocks = parse("<pre>  <span class=\"kw\">function</span> foo()</pre>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Preformatted { text, spans, .. } => {
                assert_eq!(text, "  function foo()");
                // Verify span ordering: whitespace before "function" is preserved
                assert!(spans.len() >= 2, "expected multiple spans, got {spans:?}");
                assert_eq!(spans[0].text, "  ", "leading whitespace span: {spans:?}");
                assert_eq!(spans[1].text, "function", "styled span: {spans:?}");
            }
            other => panic!("expected Preformatted, got {other:?}"),
        }
    }

    #[test]
    fn pre_with_code_child_preserves_indentation() {
        // <code> is an INLINE_STYLE_TAG (handled via the inline path).
        let blocks = parse("<pre>  <code>indented code</code>\n  <code>more</code></pre>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Preformatted { text, .. } => {
                assert!(
                    text.starts_with("  "),
                    "leading indentation must be preserved: {text:?}"
                );
                assert!(
                    text.contains("\n  more"),
                    "newline + indentation must be preserved: {text:?}"
                );
            }
            other => panic!("expected Preformatted, got {other:?}"),
        }
    }

    #[test]
    fn pre_with_multiple_styled_spans_preserves_whitespace() {
        // Multiple styled inline elements with significant whitespace between them.
        let blocks = parse("<pre>  <em>keyword</em> <strong>name</strong>(arg)</pre>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Preformatted { text, .. } => {
                assert_eq!(text, "  keyword name(arg)");
            }
            other => panic!("expected Preformatted, got {other:?}"),
        }
    }

    #[test]
    fn whitespace_between_inline_elements_normalised() {
        // Typical EPUB: "word <em>emphasis</em> word" with surrounding whitespace nodes
        let blocks = parse("<p>plain <em>italic</em> text</p>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { text, .. } => {
                // Must not have double spaces around the italic word
                assert!(!text.contains("  "), "double space found: {text:?}");
                assert_eq!(text, "plain italic text");
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    // --- parse_css_color unit tests ---

    #[test]
    fn css_color_hex6() {
        assert_eq!(parse_css_color("#ff8800"), Some([0xff, 0x88, 0x00]));
    }

    #[test]
    fn css_color_hex3() {
        assert_eq!(parse_css_color("#f80"), Some([0xff, 0x88, 0x00]));
    }

    #[test]
    fn css_color_rgb_fn() {
        assert_eq!(parse_css_color("rgb(255, 136, 0)"), Some([255, 136, 0]));
    }

    #[test]
    fn css_color_unknown_returns_none() {
        assert_eq!(parse_css_color("red"), None);
    }

    // --- parse_css_length_as_em unit tests ---

    #[test]
    fn css_length_em() {
        assert_eq!(parse_css_length_as_em("1.5em"), Some(1.5));
    }

    #[test]
    fn css_length_px() {
        assert_eq!(parse_css_length_as_em("32px"), Some(2.0));
    }

    #[test]
    fn css_length_percent() {
        assert_eq!(parse_css_length_as_em("200%"), Some(2.0));
    }

    // --- parse_inline_style unit tests ---

    #[test]
    fn inline_style_text_align_center() {
        let s = parse_inline_style("text-align: center");
        assert_eq!(s.block.text_align, Some(TextAlign::Center));
    }

    #[test]
    fn inline_style_text_align_right() {
        let s = parse_inline_style("text-align:right");
        assert_eq!(s.block.text_align, Some(TextAlign::Right));
    }

    #[test]
    fn inline_style_font_size_em() {
        let s = parse_inline_style("font-size: 2em");
        assert_eq!(s.font_size_em, Some(2.0));
    }

    #[test]
    fn inline_style_color_hex() {
        let s = parse_inline_style("color: #336699");
        assert_eq!(s.color, Some([0x33, 0x66, 0x99]));
    }

    #[test]
    fn inline_style_multiple_properties() {
        let s = parse_inline_style("text-align:center; font-size:1.2em; color:#ff0000");
        assert_eq!(s.block.text_align, Some(TextAlign::Center));
        assert_eq!(s.font_size_em, Some(1.2));
        assert_eq!(s.color, Some([255, 0, 0]));
    }

    #[test]
    fn inline_style_unknown_property_ignored() {
        let s = parse_inline_style("display:block; text-align:center");
        assert_eq!(s.block.text_align, Some(TextAlign::Center));
        assert_eq!(s.font_size_em, None);
    }

    // --- Integration-level block style tests ---

    #[test]
    fn paragraph_style_text_align_center() {
        let blocks = parse(r#"<p style="text-align:center">Centered</p>"#);
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { style, .. } => {
                assert_eq!(style.text_align, Some(TextAlign::Center));
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn heading_style_font_size() {
        let blocks = parse(r#"<h1 style="font-size:2em">Title</h1>"#);
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Heading { style, .. } => {
                assert_eq!(style.font_size_em, Some(2.0));
            }
            other => panic!("expected Heading, got {other:?}"),
        }
    }

    #[test]
    fn paragraph_without_style_has_default_block_style() {
        let blocks = parse("<p>No style</p>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { style, .. } => {
                assert_eq!(*style, BlockStyle::default());
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    // --- Span-level style= attribute tests ---

    #[test]
    fn span_style_font_weight_bold() {
        let blocks = parse(r#"<p>normal <span style="font-weight:bold">bold</span> normal</p>"#);
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { text, spans, .. } => {
                assert_eq!(text, "normal bold normal");
                assert_eq!(spans.len(), 3);
                assert!(!spans[0].style.bold);
                assert!(
                    spans[1].style.bold,
                    "span with font-weight:bold must be bold"
                );
                assert!(!spans[2].style.bold);
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn span_style_font_style_italic() {
        let blocks = parse(r#"<p><span style="font-style:italic">ital</span></p>"#);
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { spans, .. } => {
                assert!(spans[0].style.italic);
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn span_style_text_decoration_underline() {
        let blocks = parse(r#"<p><span style="text-decoration:underline">u</span></p>"#);
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { spans, .. } => {
                assert!(spans[0].style.underline);
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn span_style_text_decoration_line_through() {
        let blocks = parse(r#"<p><span style="text-decoration:line-through">s</span></p>"#);
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { spans, .. } => {
                assert!(spans[0].style.strikethrough);
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn span_style_color() {
        let blocks = parse(r#"<p><span style="color:#ff0000">red</span></p>"#);
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { spans, .. } => {
                assert_eq!(spans[0].color, Some([255, 0, 0]));
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn span_style_font_size() {
        let blocks = parse(r#"<p><span style="font-size:1.5em">big</span></p>"#);
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { spans, .. } => {
                assert_eq!(spans[0].font_size_em, Some(1.5));
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn span_style_combined() {
        let blocks = parse(
            r#"<p><span style="font-weight:bold;font-style:italic;color:#00ff00">styled</span></p>"#,
        );
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { spans, .. } => {
                assert!(spans[0].style.bold);
                assert!(spans[0].style.italic);
                assert_eq!(spans[0].color, Some([0, 255, 0]));
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn em_style_override() {
        // <em> gives italic from the tag; style= adds underline
        let blocks = parse(r#"<p><em style="text-decoration:underline">both</em></p>"#);
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { spans, .. } => {
                assert!(spans[0].style.italic, "italic from <em> tag");
                assert!(spans[0].style.underline, "underline from style=");
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn unstyled_span_still_transparent() {
        // Plain <span> without style= should produce a single span, no extra boundary
        let blocks = parse("<p><span>text</span></p>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { text, spans, .. } => {
                assert_eq!(text, "text");
                assert_eq!(spans.len(), 1);
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn span_style_in_pre_preserves_whitespace() {
        let blocks = parse(r#"<pre>  <span style="color:#ff0000">red code</span>  more</pre>"#);
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Preformatted { text, spans, .. } => {
                assert_eq!(text, "  red code  more");
                // The styled span should carry the color
                let red_span = spans.iter().find(|s| s.text == "red code").unwrap();
                assert_eq!(red_span.color, Some([255, 0, 0]));
            }
            other => panic!("expected Preformatted, got {other:?}"),
        }
    }

    #[test]
    fn parse_inline_style_font_weight_numeric() {
        let s = parse_inline_style("font-weight: 700");
        assert!(s.inline.bold, "font-weight:700 should be bold");
        let s = parse_inline_style("font-weight: 400");
        assert!(!s.inline.bold, "font-weight:400 should not be bold");
    }

    #[test]
    fn parse_inline_style_font_family_monospace() {
        let s = parse_inline_style("font-family: 'Courier New', monospace");
        assert!(s.inline.monospaced);
    }

    // --- Stylesheet integration tests ---

    use crate::content::parse_css;

    fn parse_with_css(html: &str, css: &str) -> Vec<ContentBlock> {
        let sheet = parse_css(css);
        parse_xhtml(html.as_bytes(), "OEBPS/Text/ch1.xhtml", &sheet, &mut |_| {
            None
        })
    }

    #[test]
    fn stylesheet_class_applies_text_align() {
        let blocks = parse_with_css(
            r#"<p class="verse">text</p>"#,
            ".verse { text-align: center; }",
        );
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { style, .. } => {
                assert_eq!(style.text_align, Some(TextAlign::Center));
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn stylesheet_tag_applies_style() {
        let blocks = parse_with_css("<p>text</p>", "p { text-align: center; }");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { style, .. } => {
                assert_eq!(style.text_align, Some(TextAlign::Center));
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn stylesheet_tag_and_class_applies_style() {
        let blocks = parse_with_css(
            r#"<p class="indent">text</p>"#,
            "p.indent { margin-top: 0.5em; }",
        );
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { style, .. } => {
                assert_eq!(style.margin_top_em, Some(0.5));
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn inline_style_overrides_stylesheet() {
        let blocks = parse_with_css(
            r#"<p class="verse" style="text-align:left">text</p>"#,
            ".verse { text-align: center; }",
        );
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { style, .. } => {
                assert_eq!(
                    style.text_align,
                    Some(TextAlign::Left),
                    "inline style= must override stylesheet"
                );
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn stylesheet_justify_falls_back_to_left() {
        let blocks = parse_with_css("<p>text</p>", "p { text-align: justify; }");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { style, .. } => {
                assert_eq!(style.text_align, Some(TextAlign::Left));
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn stylesheet_span_class_applies_inline_style() {
        let blocks = parse_with_css(
            r#"<p><span class="bold">text</span></p>"#,
            ".bold { font-weight: bold; }",
        );
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { spans, .. } => {
                assert!(
                    spans[0].style.bold,
                    "stylesheet .bold should make span bold"
                );
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn stylesheet_heading_applies_style() {
        let blocks = parse_with_css(
            r#"<h1 class="chapter-heading">Title</h1>"#,
            ".chapter-heading { text-align: center; }",
        );
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Heading { style, .. } => {
                assert_eq!(style.text_align, Some(TextAlign::Center));
            }
            other => panic!("expected Heading, got {other:?}"),
        }
    }

    #[test]
    fn empty_stylesheet_identical_to_no_stylesheet() {
        let with_empty = parse_with_css("<p>text</p>", "");
        let without = parse("<p>text</p>");
        assert_eq!(with_empty.len(), without.len());
        match (&with_empty[0], &without[0]) {
            (
                ContentBlock::Paragraph {
                    text: t1,
                    style: s1,
                    ..
                },
                ContentBlock::Paragraph {
                    text: t2,
                    style: s2,
                    ..
                },
            ) => {
                assert_eq!(t1, t2);
                assert_eq!(s1, s2);
            }
            _ => panic!("expected matching Paragraphs"),
        }
    }

    #[test]
    fn stylesheet_span_color() {
        let blocks = parse_with_css(
            r#"<p><span class="red">text</span></p>"#,
            ".red { color: #ff0000; }",
        );
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { spans, .. } => {
                assert_eq!(spans[0].color, Some([255, 0, 0]));
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn stylesheet_inline_tag_with_class() {
        let blocks = parse_with_css(
            r#"<p><em class="special">text</em></p>"#,
            ".special { text-decoration: underline; }",
        );
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { spans, .. } => {
                assert!(spans[0].style.italic, "italic from <em> tag");
                assert!(spans[0].style.underline, "underline from stylesheet class");
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn parses_inline_svg() {
        let blocks = parse("<svg><circle cx=\"50\" cy=\"50\" r=\"40\" fill=\"red\"/></svg>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Svg {
                content,
                alt,
                style,
                ..
            } => {
                assert!(!content.is_empty());
                assert!(content.contains("<circle"));
                assert!(content.contains("cx=\"50\""));
                assert!(content.contains("fill=\"red\""));
                assert_eq!(alt, "");
                assert_eq!(*style, BlockStyle::default());
            }
            other => panic!("expected Svg, got {other:?}"),
        }
    }

    #[test]
    fn parses_inline_svg_with_text_content() {
        let blocks = parse("<svg><text x=\"10\" y=\"20\">Hello SVG</text></svg>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Svg { content, .. } => {
                assert!(content.contains("<text"));
                assert!(content.contains("Hello SVG"));
                assert!(content.contains("x=\"10\""));
                assert!(content.contains("y=\"20\""));
            }
            other => panic!("expected Svg, got {other:?}"),
        }
    }

    #[test]
    fn parses_svg_image_placeholder() {
        let blocks = parse("<img src=\"diagram.svg\" alt=\"Diagram\">");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { text, .. } => {
                // Image placeholder gets converted to fallback paragraph when resolution fails
                assert_eq!(text, "[Diagram]");
            }
            other => panic!("expected fallback Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn parses_raster_image_placeholder() {
        let blocks = parse("<img src=\"photo.jpg\" alt=\"Photo\">");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { text, .. } => {
                // Image placeholder gets converted to fallback paragraph when resolution fails
                assert_eq!(text, "[Photo]");
            }
            other => panic!("expected fallback Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn parses_mixed_svg_and_images() {
        let blocks = parse(
            "<svg><circle r=\"10\"/></svg><img src=\"icon.svg\" alt=\"Icon\"><img src=\"photo.jpg\" alt=\"Photo\">",
        );
        assert_eq!(blocks.len(), 3);

        // First should be inline SVG
        match &blocks[0] {
            ContentBlock::Svg { content, .. } => {
                assert!(!content.is_empty());
                assert!(content.contains("<circle"));
            }
            other => panic!("expected inline Svg, got {other:?}"),
        }

        // Second should be fallback paragraph for SVG image
        match &blocks[1] {
            ContentBlock::Paragraph { text, .. } => {
                assert_eq!(text, "[Icon]");
            }
            other => panic!("expected fallback Paragraph for SVG image, got {other:?}"),
        }

        // Third should be fallback paragraph for raster image
        match &blocks[2] {
            ContentBlock::Paragraph { text, .. } => {
                assert_eq!(text, "[Photo]");
            }
            other => panic!("expected fallback Paragraph for raster image, got {other:?}"),
        }
    }

    #[test]
    fn svg_with_css_class_styling() {
        let blocks = parse_with_css(
            r#"<svg class="centered"><rect x="0" y="0" width="50" height="50"/></svg>"#,
            ".centered { text-align: center; }",
        );
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Svg { content, style, .. } => {
                assert!(content.contains("<rect"));
                assert_eq!(style.text_align, Some(TextAlign::Center));
            }
            other => panic!("expected Svg with styling, got {other:?}"),
        }
    }

    #[test]
    fn svg_with_inline_style() {
        let blocks = parse("<svg style=\"text-align: right\"><circle r=\"20\"/></svg>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Svg { content, style, .. } => {
                assert!(content.contains("<circle"));
                assert_eq!(style.text_align, Some(TextAlign::Right));
            }
            other => panic!("expected Svg with inline style, got {other:?}"),
        }
    }

    #[test]
    fn svg_viewbox_aspect_ratio_square() {
        let blocks = parse(r#"<svg viewBox="0 0 100 100"><circle r="40"/></svg>"#);
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Svg { aspect_ratio, .. } => {
                assert!(
                    aspect_ratio.is_some_and(|ar| (ar - 1.0).abs() < 0.001),
                    "expected aspect_ratio ≈ 1.0, got {aspect_ratio:?}"
                );
            }
            other => panic!("expected Svg, got {other:?}"),
        }
    }

    #[test]
    fn svg_viewbox_aspect_ratio_wide() {
        // viewBox="0 0 200 100" → w=200, h=100, ratio=2.0
        let blocks = parse(r#"<svg viewBox="0 0 200 100"><rect width="200" height="100"/></svg>"#);
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Svg { aspect_ratio, .. } => {
                assert!(
                    aspect_ratio.is_some_and(|ar| (ar - 2.0).abs() < 0.001),
                    "expected aspect_ratio ≈ 2.0, got {aspect_ratio:?}"
                );
            }
            other => panic!("expected Svg, got {other:?}"),
        }
    }

    #[test]
    fn svg_without_viewbox_has_no_aspect_ratio() {
        let blocks = parse(r#"<svg width="100" height="100"><circle r="40"/></svg>"#);
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Svg { aspect_ratio, .. } => {
                assert!(
                    aspect_ratio.is_none(),
                    "expected no aspect_ratio, got {aspect_ratio:?}"
                );
            }
            other => panic!("expected Svg, got {other:?}"),
        }
    }

    #[test]
    fn svg_with_relative_image_path() {
        let blocks = parse(
            r#"<svg width="200" height="200" viewBox="0 0 200 200">
  <image width="200" height="200" xlink:href="../media/Cover.png"/>
</svg>"#,
        );
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Svg { content, .. } => {
                assert!(content.contains("../media/Cover.png"));
                assert!(content.contains("<image"));
                assert!(content.contains("xlink:href"));
            }
            _ => panic!("Expected Svg block"),
        }
    }

    #[test]
    fn svg_image_resolution_with_relative_paths() {
        let svg_content = r#"<svg width="200" height="200">
  <image width="200" height="200" xlink:href="../media/Cover.png"/>
</svg>"#;

        let chapter_href = "OEBPS/Text/chapter1.xhtml";

        // Mock resolver that simulates finding the image at the resolved path
        let resolved_content = resolve_svg_images(svg_content, chapter_href, &mut |path| {
            if path == "OEBPS/media/Cover.png" {
                Some((vec![1, 2, 3, 4], "image/png".to_string()))
            } else {
                None
            }
        });

        // Should have converted the relative path to a data URI
        assert!(resolved_content.contains("data:image/png;base64,"));
        assert!(!resolved_content.contains("../media/Cover.png"));
    }

    #[test]
    fn svg_image_resolution_plain_relative_path() {
        // A plain relative href (no leading ./ or ../) must still be resolved
        // against the chapter's directory, not passed through unchanged.
        let svg_content = r#"<svg width="200" height="200">
  <image width="200" height="200" xlink:href="media/Cover.png"/>
</svg>"#;

        let chapter_href = "OEBPS/Text/chapter1.xhtml";

        let resolved_content = resolve_svg_images(svg_content, chapter_href, &mut |path| {
            if path == "OEBPS/Text/media/Cover.png" {
                Some((vec![1, 2, 3, 4], "image/png".to_string()))
            } else {
                None
            }
        });

        assert!(resolved_content.contains("data:image/png;base64,"));
    }

    #[test]
    fn svg_image_resolution_fallback_on_missing() {
        let svg_content = r#"<svg width="200" height="200">
  <image width="200" height="200" xlink:href="../media/Missing.png"/>
</svg>"#;

        let chapter_href = "OEBPS/Text/chapter1.xhtml";

        // Mock resolver that returns None for all paths
        let resolved_content = resolve_svg_images(svg_content, chapter_href, &mut |_| None);

        // Should preserve original href when resolution fails
        assert!(resolved_content.contains("../media/Missing.png"));
        assert!(!resolved_content.contains("data:"));
    }

    #[test]
    fn svg_relative_path_resolution_realistic_scenario() {
        // Test the actual scenario: chapter in OEBPS/Text/ referencing image in OEBPS/media/
        let svg_content = r#"<svg width="200" height="200">
  <image width="200" height="200" xlink:href="../media/Cover.png"/>
</svg>"#;

        let chapter_href = "OEBPS/Text/chapter1.xhtml";

        // Mock resolver that simulates the EPUB structure
        let resolved_content =
            resolve_svg_images(svg_content, chapter_href, &mut |path| match path {
                "OEBPS/media/Cover.png" => Some((vec![1, 2, 3, 4], "image/png".to_string())),
                _ => None,
            });

        // Verify the relative path was resolved correctly
        assert!(resolved_content.contains("data:image/png;base64,"));
        assert!(!resolved_content.contains("../media/Cover.png"));

        // Verify the SVG structure is preserved
        assert!(resolved_content.contains("<svg"));
        assert!(resolved_content.contains("<image"));
        assert!(resolved_content.contains("width=\"200\""));
        assert!(resolved_content.contains("height=\"200\""));
    }

    #[test]
    fn svg_relative_path_resolution_deeply_nested() {
        // Test deeper nesting: chapter in OEBPS/Text/Subsection/ referencing image in OEBPS/media/
        let svg_content = r#"<svg width="200" height="200">
  <image width="200" height="200" xlink:href="../../media/Cover.png"/>
</svg>"#;

        let chapter_href = "OEBPS/Text/Subsection/chapter1.xhtml";

        let resolved_content =
            resolve_svg_images(svg_content, chapter_href, &mut |path| match path {
                "OEBPS/media/Cover.png" => Some((vec![1, 2, 3, 4], "image/png".to_string())),
                _ => None,
            });

        // Should resolve correctly
        assert!(resolved_content.contains("data:image/png;base64,"));
        assert!(!resolved_content.contains("../../media/Cover.png"));
    }

    #[test]
    fn svg_image_same_dir_as_chapter() {
        // A plain relative href (no ./ or ../) in an SVG <image> must be resolved
        // against the chapter's directory, the same as any other relative reference.
        let svg_content = r#"<svg width="200" height="200">
  <image width="200" height="200" xlink:href="cover.jpg"/>
</svg>"#;
        let chapter_href = "OEBPS/content.xhtml";
        let resolved_content = resolve_svg_images(svg_content, chapter_href, &mut |path| {
            if path == "OEBPS/cover.jpg" {
                Some((vec![1, 2, 3, 4], "image/jpeg".to_string()))
            } else {
                None
            }
        });
        assert!(
            resolved_content.contains("data:image/jpeg;base64,"),
            "plain relative SVG image href was not resolved against chapter directory"
        );
    }

    #[test]
    fn svg_with_embedded_image_tag() {
        let blocks = parse(
            r#"<svg width="200" height="200" viewBox="0 0 200 200">
  <image width="200" height="200" xlink:href="image.svg"/>
</svg>"#,
        );
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Svg { content, .. } => {
                assert!(content.contains("<svg"));
                assert!(content.contains("width=\"200\""));
                assert!(content.contains("height=\"200\""));
                assert!(content.contains("<image"));
                assert!(content.contains("xlink:href=\"image.svg\""));
                assert!(content.contains("</svg>"));
            }
            other => panic!("expected Svg, got {other:?}"),
        }
    }

    #[test]
    fn empty_svg_is_ignored() {
        let blocks = parse("<svg></svg>");
        assert_eq!(blocks.len(), 0); // Empty SVG should be filtered out
    }

    #[test]
    fn heading_with_id_emits_anchor_before_heading() {
        let blocks = parse(r#"<h2 id="section-1">Section One</h2>"#);
        // Anchor precedes the Heading
        assert_eq!(blocks.len(), 2);
        match &blocks[0] {
            ContentBlock::Anchor { id } => assert_eq!(id, "section-1"),
            other => panic!("expected Anchor, got {other:?}"),
        }
        match &blocks[1] {
            ContentBlock::Heading { level: 2, text, .. } => assert_eq!(text, "Section One"),
            other => panic!("expected Heading h2, got {other:?}"),
        }
    }

    #[test]
    fn heading_without_id_emits_no_anchor() {
        let blocks = parse("<h2>Section One</h2>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Heading { level: 2, .. } => {}
            other => panic!("expected Heading h2, got {other:?}"),
        }
    }

    #[test]
    fn section_with_id_emits_anchor_before_children() {
        let blocks =
            parse(r#"<section id="ch1"><h2>Chapter Title</h2><p>Body text.</p></section>"#);
        // Anchor, then Heading, then Paragraph
        assert_eq!(blocks.len(), 3, "blocks: {blocks:?}");
        match &blocks[0] {
            ContentBlock::Anchor { id } => assert_eq!(id, "ch1"),
            other => panic!("expected Anchor, got {other:?}"),
        }
        match &blocks[1] {
            ContentBlock::Heading { level: 2, .. } => {}
            other => panic!("expected Heading h2, got {other:?}"),
        }
        match &blocks[2] {
            ContentBlock::Paragraph { text, .. } => assert_eq!(text, "Body text."),
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn figure_with_caption() {
        let blocks = parse(
            r#"<figure><img src="photo.jpg" alt="A cat"/><figcaption>A sleeping cat.</figcaption></figure>"#,
        );
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Figure {
                blocks,
                caption,
                caption_text,
            } => {
                assert_eq!(
                    blocks.len(),
                    1,
                    "expected one inner block (image placeholder)"
                );
                assert_eq!(caption_text, "A sleeping cat.");
                assert!(!caption.is_empty(), "caption spans should be non-empty");
                assert_eq!(caption[0].text, "A sleeping cat.");
            }
            other => panic!("expected Figure, got {other:?}"),
        }
    }

    #[test]
    fn figure_without_caption() {
        let blocks = parse(r#"<figure><img src="photo.jpg" alt="An image"/></figure>"#);
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Figure {
                blocks,
                caption,
                caption_text,
            } => {
                assert_eq!(blocks.len(), 1);
                assert!(caption.is_empty());
                assert!(caption_text.is_empty());
            }
            other => panic!("expected Figure, got {other:?}"),
        }
    }

    #[test]
    fn figure_caption_with_styled_span() {
        let blocks = parse(
            r#"<figure><img src="x.jpg"/><figcaption>See <em>figure one</em>.</figcaption></figure>"#,
        );
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Figure {
                caption,
                caption_text,
                ..
            } => {
                assert_eq!(caption_text, "See figure one.");
                // Three spans: "See ", "figure one" (italic), "."
                assert_eq!(caption.len(), 3);
                assert!(!caption[0].style.italic);
                assert_eq!(caption[0].text, "See ");
                assert!(caption[1].style.italic);
                assert_eq!(caption[1].text, "figure one");
                assert!(!caption[2].style.italic);
                assert_eq!(caption[2].text, ".");
            }
            other => panic!("expected Figure, got {other:?}"),
        }
    }

    #[test]
    fn figcaption_outside_figure_becomes_paragraph() {
        // Malformed HTML: figcaption not inside a figure — fallback to paragraph
        let blocks = parse("<figcaption>Orphan caption.</figcaption>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { text, .. } => assert_eq!(text, "Orphan caption."),
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn image_inside_figure_is_resolved() {
        // Images inside <figure> must be resolved, not left as empty placeholders.
        let png_data = vec![0x89, 0x50, 0x4E, 0x47];
        let data_clone = png_data.clone();
        let blocks = parse_xhtml(
            b"<figure><img src=\"../Images/photo.png\" alt=\"A photo\"/><figcaption>Caption</figcaption></figure>",
            "OEBPS/Text/ch1.xhtml",
            &StyleSheet::empty(),
            &mut move |path| {
                if path == "OEBPS/Images/photo.png" {
                    Some((data_clone.clone(), "image/png".to_string()))
                } else {
                    None
                }
            },
        );
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Figure {
                blocks,
                caption_text,
                ..
            } => {
                assert_eq!(caption_text, "Caption");
                assert_eq!(blocks.len(), 1);
                match &blocks[0] {
                    ContentBlock::Image { alt, data, .. } => {
                        assert_eq!(alt, "A photo");
                        assert_eq!(data, &png_data);
                    }
                    other => panic!("expected Image inside Figure, got {other:?}"),
                }
            }
            other => panic!("expected Figure, got {other:?}"),
        }
    }

    #[test]
    fn standalone_anchor_at_body_level_becomes_paragraph() {
        // A bare <a href="..."> directly in <body> (not wrapped in <p>) must not be dropped.
        let blocks = parse(
            r#"<figure><img src="x.png" alt="img"/></figure><a href="desc.xhtml">Follow for extended description</a>"#,
        );
        // Should produce: Figure + Paragraph with link text
        assert_eq!(
            blocks.len(),
            2,
            "expected Figure + Paragraph, got {blocks:?}"
        );
        match &blocks[1] {
            ContentBlock::Paragraph { text, spans, .. } => {
                assert_eq!(text, "Follow for extended description");
                assert_eq!(spans.len(), 1);
                assert_eq!(spans[0].link.as_deref(), Some("desc.xhtml"));
            }
            other => panic!("expected Paragraph for anchor, got {other:?}"),
        }
    }
}
