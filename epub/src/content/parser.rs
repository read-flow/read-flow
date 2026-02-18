use std::cell::RefCell;

use html5ever::tokenizer::BufferQueue;
use html5ever::tokenizer::Tag;
use html5ever::tokenizer::TagKind;
use html5ever::tokenizer::Token;
use html5ever::tokenizer::TokenSink;
use html5ever::tokenizer::TokenSinkResult;
use html5ever::tokenizer::Tokenizer;
use html5ever::tokenizer::TokenizerOpts;

use super::block::ContentBlock;
use super::block::ListItem;
use super::resolve::base_dir;
use super::resolve::guess_media_type;
use super::resolve::resolve_href;

/// An element on the parsing stack.
struct StackEntry {
    tag: String,
    text: String,
    children: Vec<ContentBlock>,
    list_items: Vec<ListItem>,
    /// For `<ol>` elements, the start attribute value.
    ol_start: u32,
}

impl StackEntry {
    fn new(tag: &str) -> Self {
        StackEntry {
            tag: tag.to_string(),
            text: String::new(),
            children: Vec::new(),
            list_items: Vec::new(),
            ol_start: 1,
        }
    }
}

/// A pending image to be resolved after parsing.
struct PendingImage {
    /// Resolved zip path for the image.
    resolved_path: String,
    /// Alt text from the `<img>` element.
    alt: String,
}

/// State accumulated during tokenization.
struct SinkState {
    stack: Vec<StackEntry>,
    output: Vec<ContentBlock>,
    skip_depth: usize,
    base_href: String,
    /// Images collected during parsing, keyed to their position in the output.
    /// Each entry is `(block_index_path, pending_image)` where block_index_path
    /// identifies where in the output tree the placeholder lives.
    pending_images: Vec<(usize, PendingImage)>,
    /// Counter for placeholder images inserted into the output.
    image_counter: usize,
}

/// Token sink that builds `Vec<ContentBlock>` from XHTML tokens.
struct ContentSink {
    state: RefCell<SinkState>,
}

impl ContentSink {
    fn new(base_href: &str) -> Self {
        ContentSink {
            state: RefCell::new(SinkState {
                stack: vec![StackEntry::new("root")],
                output: Vec::new(),
                skip_depth: 0,
                base_href: base_href.to_string(),
                pending_images: Vec::new(),
                image_counter: 0,
            }),
        }
    }

    fn into_blocks_and_pending(self) -> (Vec<ContentBlock>, Vec<(usize, PendingImage)>) {
        let mut state = self.state.into_inner();
        if let Some(root) = state.stack.pop() {
            state.output.extend(root.children);
        }
        (state.output, state.pending_images)
    }
}

const SKIP_TAGS: &[&str] = &["head", "style", "script", "title"];
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param",
    "source", "track", "wbr",
];
const TRANSPARENT_TAGS: &[&str] = &[
    "div", "section", "article", "body", "html", "span", "a", "em", "strong", "b", "i", "u",
    "small", "sub", "sup", "mark", "abbr", "cite", "del", "ins", "s", "nav", "main", "header",
    "footer", "aside", "figure", "figcaption", "details", "summary", "table", "thead", "tbody",
    "tfoot", "tr", "td", "th", "dl", "dt", "dd",
];

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

                if state.skip_depth > 0 {
                    if !self_closing && !VOID_ELEMENTS.contains(&tag_name) {
                        state.skip_depth += 1;
                    }
                    return TokenSinkResult::Continue;
                }

                if SKIP_TAGS.contains(&tag_name) {
                    state.skip_depth = 1;
                    return TokenSinkResult::Continue;
                }

                if tag_name == "br" {
                    if let Some(entry) = state.stack.last_mut() {
                        entry.text.push('\n');
                    }
                    return TokenSinkResult::Continue;
                }

                if tag_name == "hr" {
                    if let Some(entry) = state.stack.last_mut() {
                        entry.children.push(ContentBlock::HorizontalRule);
                    }
                    return TokenSinkResult::Continue;
                }

                if tag_name == "img" {
                    let src = find_attr(attrs, "src");
                    let alt = find_attr(attrs, "alt").unwrap_or_default();

                    if let Some(src) = src {
                        let base = base_dir(&state.base_href);
                        let resolved = resolve_href(base, &src);

                        // Insert a placeholder image block and record it for later resolution
                        let id = state.image_counter;
                        state.image_counter += 1;
                        let placeholder = ContentBlock::Image {
                            alt: alt.clone(),
                            data: Vec::new(),
                            media_type: guess_media_type(&resolved),
                        };
                        if let Some(entry) = state.stack.last_mut() {
                            entry.children.push(placeholder);
                        }
                        state.pending_images.push((
                            id,
                            PendingImage {
                                resolved_path: resolved,
                                alt,
                            },
                        ));
                    } else if !alt.is_empty() {
                        if let Some(entry) = state.stack.last_mut() {
                            entry.children.push(ContentBlock::Paragraph {
                                text: format!("[{alt}]"),
                            });
                        }
                    }
                    return TokenSinkResult::Continue;
                }

                let mut entry = StackEntry::new(tag_name);

                if tag_name == "ol" {
                    if let Some(start_str) = find_attr(attrs, "start") {
                        entry.ol_start = start_str.parse().unwrap_or(1);
                    }
                }

                state.stack.push(entry);

                if self_closing {
                    let _ = state.stack.pop();
                }
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
                    state.stack.pop().unwrap()
                } else {
                    return TokenSinkResult::Continue;
                };

                let text = entry.text.trim().to_string();
                let block = match tag_name {
                    "h1" => Some(ContentBlock::Heading { level: 1, text }),
                    "h2" => Some(ContentBlock::Heading { level: 2, text }),
                    "h3" => Some(ContentBlock::Heading { level: 3, text }),
                    "h4" => Some(ContentBlock::Heading { level: 4, text }),
                    "h5" => Some(ContentBlock::Heading { level: 5, text }),
                    "h6" => Some(ContentBlock::Heading { level: 6, text }),
                    "p" => {
                        if text.is_empty() && entry.children.is_empty() {
                            None
                        } else if entry.children.is_empty() {
                            Some(ContentBlock::Paragraph { text })
                        } else {
                            let mut blocks = Vec::new();
                            if !text.is_empty() {
                                blocks.push(ContentBlock::Paragraph { text });
                            }
                            blocks.extend(entry.children);
                            if let Some(parent) = state.stack.last_mut() {
                                parent.children.extend(blocks);
                            } else {
                                state.output.extend(blocks);
                            }
                            None
                        }
                    }
                    "pre" | "code" => {
                        if tag_name == "pre"
                            || !state.stack.iter().any(|e| e.tag == "pre")
                        {
                            if !entry.text.is_empty() {
                                Some(ContentBlock::Preformatted { text: entry.text })
                            } else {
                                None
                            }
                        } else {
                            if let Some(parent) = state.stack.last_mut() {
                                parent.text.push_str(&entry.text);
                            }
                            None
                        }
                    }
                    "blockquote" => {
                        let mut children = entry.children;
                        if !text.is_empty() {
                            children.insert(0, ContentBlock::Paragraph { text });
                        }
                        if !children.is_empty() {
                            Some(ContentBlock::BlockQuote { children })
                        } else {
                            None
                        }
                    }
                    "ul" => {
                        if !entry.list_items.is_empty() {
                            Some(ContentBlock::UnorderedList {
                                items: entry.list_items,
                            })
                        } else {
                            None
                        }
                    }
                    "ol" => {
                        if !entry.list_items.is_empty() {
                            Some(ContentBlock::OrderedList {
                                start: entry.ol_start,
                                items: entry.list_items,
                            })
                        } else {
                            None
                        }
                    }
                    "li" => {
                        if let Some(parent) = state.stack.last_mut() {
                            parent.list_items.push(ListItem { text });
                        }
                        None
                    }
                    _ if TRANSPARENT_TAGS.contains(&tag_name) => {
                        promote_to_parent(&mut state, text, entry.children, entry.list_items);
                        None
                    }
                    _ => {
                        promote_to_parent(&mut state, text, entry.children, entry.list_items);
                        None
                    }
                };

                if let Some(block) = block {
                    if let Some(parent) = state.stack.last_mut() {
                        parent.children.push(block);
                    } else {
                        state.output.push(block);
                    }
                }
            }

            Token::CharacterTokens(text) => {
                if state.skip_depth > 0 {
                    return TokenSinkResult::Continue;
                }
                if let Some(entry) = state.stack.last_mut() {
                    entry.text.push_str(&text);
                }
            }

            _ => {}
        }

        TokenSinkResult::Continue
    }
}

fn promote_to_parent(
    state: &mut SinkState,
    text: String,
    children: Vec<ContentBlock>,
    list_items: Vec<ListItem>,
) {
    if let Some(parent) = state.stack.last_mut() {
        if !text.is_empty() {
            if !parent.text.is_empty() && !parent.text.ends_with(char::is_whitespace) {
                parent.text.push(' ');
            }
            parent.text.push_str(&text);
        }
        parent.children.extend(children);
        parent.list_items.extend(list_items);
    } else {
        if !text.is_empty() {
            state.output.push(ContentBlock::Paragraph { text });
        }
        state.output.extend(children);
    }
}

fn find_attr(attrs: &[html5ever::Attribute], name: &str) -> Option<String> {
    let target = html5ever::LocalName::from(name);
    attrs
        .iter()
        .find(|a| a.name.local == target)
        .map(|a| a.value.to_string())
}

/// Parse XHTML content into structured content blocks.
///
/// - `xhtml`: raw XHTML bytes
/// - `chapter_href`: the zip path of this chapter (e.g. `"OEBPS/Text/ch1.xhtml"`),
///   used to resolve relative image paths
/// - `resolve_image`: callback that takes a resolved zip path and returns
///   `(data, media_type)` or `None` if the resource can't be found
pub fn parse_xhtml(
    xhtml: &[u8],
    chapter_href: &str,
    resolve_image: &mut dyn FnMut(&str) -> Option<(Vec<u8>, String)>,
) -> Vec<ContentBlock> {
    let html_str = String::from_utf8_lossy(xhtml);

    let sink = ContentSink::new(chapter_href);

    let tokenizer = Tokenizer::new(sink, TokenizerOpts::default());

    let buf = BufferQueue::default();
    buf.push_back(html5ever::tendril::StrTendril::from(html_str.as_ref()));
    let _ = tokenizer.feed(&buf);
    tokenizer.end();

    let (mut blocks, pending_images) = tokenizer.sink.into_blocks_and_pending();

    // Resolve pending images by walking the block tree
    if !pending_images.is_empty() {
        resolve_images(&mut blocks, &pending_images, resolve_image);
    }

    blocks
}

/// Walk the block tree and resolve placeholder Image blocks.
fn resolve_images(
    blocks: &mut Vec<ContentBlock>,
    pending: &[(usize, PendingImage)],
    resolve_image: &mut dyn FnMut(&str) -> Option<(Vec<u8>, String)>,
) {
    for block in blocks.iter_mut() {
        match block {
            ContentBlock::Image {
                alt,
                data,
                media_type,
            } if data.is_empty() => {
                // This is a placeholder — find matching pending image
                if let Some((_, img)) = pending.iter().find(|(_, img)| img.alt == *alt) {
                    if let Some((resolved_data, resolved_mt)) =
                        resolve_image(&img.resolved_path)
                    {
                        *data = resolved_data;
                        *media_type = resolved_mt;
                    } else {
                        // Replace with alt-text paragraph fallback
                        if !alt.is_empty() {
                            *block = ContentBlock::Paragraph {
                                text: format!("[{}]", alt),
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
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(html: &str) -> Vec<ContentBlock> {
        parse_xhtml(html.as_bytes(), "OEBPS/Text/ch1.xhtml", &mut |_| None)
    }

    #[test]
    fn parses_paragraph() {
        let blocks = parse("<html><body><p>Hello world</p></body></html>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { text } => assert_eq!(text, "Hello world"),
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn parses_headings() {
        let blocks = parse("<h1>Title</h1><h2>Subtitle</h2><h3>Section</h3>");
        assert_eq!(blocks.len(), 3);
        match &blocks[0] {
            ContentBlock::Heading { level: 1, text } => assert_eq!(text, "Title"),
            other => panic!("expected Heading h1, got {other:?}"),
        }
        match &blocks[1] {
            ContentBlock::Heading { level: 2, text } => assert_eq!(text, "Subtitle"),
            other => panic!("expected Heading h2, got {other:?}"),
        }
        match &blocks[2] {
            ContentBlock::Heading { level: 3, text } => assert_eq!(text, "Section"),
            other => panic!("expected Heading h3, got {other:?}"),
        }
    }

    #[test]
    fn skips_empty_paragraphs() {
        let blocks = parse("<p></p><p>  </p><p>content</p>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { text } => assert_eq!(text, "content"),
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
                    ContentBlock::Paragraph { text } => assert_eq!(text, "Quoted text"),
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
            ContentBlock::Preformatted { text } => {
                assert!(text.contains("code here"));
                assert!(text.contains("indented"));
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
    fn flattens_inline_formatting() {
        let blocks = parse("<p>This is <strong>bold</strong> and <em>italic</em> text</p>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { text } => {
                assert!(text.contains("bold"));
                assert!(text.contains("italic"));
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn transparent_containers_promote_children() {
        let blocks = parse("<div><p>inside div</p></div><section><p>inside section</p></section>");
        assert_eq!(blocks.len(), 2);
        match &blocks[0] {
            ContentBlock::Paragraph { text } => assert_eq!(text, "inside div"),
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
            ContentBlock::Paragraph { text } => assert_eq!(text, "[A picture]"),
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
            ContentBlock::Paragraph { text } => assert_eq!(text, "Keep"),
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
            ContentBlock::Heading { level: 1, text } => assert_eq!(text, "Chapter One"),
            other => panic!("expected Heading h1, got {other:?}"),
        }
        match &blocks[1] {
            ContentBlock::Paragraph { text } => assert_eq!(text, "Body text here."),
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }

    #[test]
    fn br_inserts_newline() {
        let blocks = parse("<p>line one<br/>line two</p>");
        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            ContentBlock::Paragraph { text } => {
                assert!(text.contains("line one"));
                assert!(text.contains("line two"));
                assert!(text.contains('\n'));
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }
    }
}
