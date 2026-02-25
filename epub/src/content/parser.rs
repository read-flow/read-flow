use std::cell::RefCell;

use html5ever::tokenizer::BufferQueue;
use html5ever::tokenizer::Tag;
use html5ever::tokenizer::TagKind;
use html5ever::tokenizer::Token;
use html5ever::tokenizer::TokenSink;
use html5ever::tokenizer::TokenSinkResult;
use html5ever::tokenizer::Tokenizer;
use html5ever::tokenizer::TokenizerOpts;

use super::block::BlockStyle;
use super::block::ContentBlock;
use super::block::InlineStyle;
use super::block::ListItem;
use super::block::TableCell;
use super::block::TextAlign;
use super::block::TextSpan;
use super::resolve::base_dir;
use super::resolve::guess_media_type;
use super::resolve::resolve_href;
use super::stylesheet::StyleSheet;

/// An element on the parsing stack.
struct StackEntry {
    tag: String,
    text: String,
    children: Vec<ContentBlock>,
    list_items: Vec<ListItem>,
    /// For `<ol>` elements, the start attribute value.
    ol_start: u32,
    /// Accumulated styled spans for this element.
    spans: Vec<TextSpan>,
    /// The inherited inline style at this stack level.
    inline_style: InlineStyle,
    /// Href from an enclosing `<a>` element, inherited by child entries.
    link: Option<String>,
    /// Accumulated rows for `<table>` elements; each row is a `Vec<TableCell>`.
    table_rows: Vec<Vec<TableCell>>,
    /// Accumulated cells for the current `<tr>` element.
    table_cells: Vec<TableCell>,
    /// The `id` HTML attribute of this element, used for footnote anchoring.
    element_id: Option<String>,
    /// True when this is an `<aside epub:type="footnote">` element.
    is_footnote: bool,
    /// True when this element is a container of footnote items
    /// (`class="footnotes"`, `role="doc-endnotes"`, `epub:type="endnotes"`, etc.).
    is_footnote_container: bool,
    /// Block-level style parsed from the element's `style="..."` attribute.
    block_style: BlockStyle,
    /// Per-span color override from a `style="color:..."` attribute on an inline element.
    span_color: Option<[u8; 3]>,
    /// Per-span font-size multiplier from a `style="font-size:..."` attribute on an inline element.
    span_font_size_em: Option<f32>,
    /// Accumulated SVG XML content for `<svg>` elements.
    svg_content: String,
}

impl StackEntry {
    fn new(tag: &str) -> Self {
        StackEntry {
            tag: tag.to_string(),
            text: String::new(),
            children: Vec::new(),
            list_items: Vec::new(),
            ol_start: 1,
            spans: Vec::new(),
            inline_style: InlineStyle::default(),
            link: None,
            table_rows: Vec::new(),
            table_cells: Vec::new(),
            element_id: None,
            is_footnote: false,
            is_footnote_container: false,
            block_style: BlockStyle::default(),
            span_color: None,
            span_font_size_em: None,
            svg_content: String::new(),
        }
    }

    fn new_with_style(tag: &str, style: InlineStyle, link: Option<String>) -> Self {
        StackEntry {
            tag: tag.to_string(),
            text: String::new(),
            children: Vec::new(),
            list_items: Vec::new(),
            ol_start: 1,
            spans: Vec::new(),
            inline_style: style,
            link,
            table_rows: Vec::new(),
            table_cells: Vec::new(),
            element_id: None,
            is_footnote: false,
            is_footnote_container: false,
            block_style: BlockStyle::default(),
            span_color: None,
            span_font_size_em: None,
            svg_content: String::new(),
        }
    }

    /// Flush any accumulated text into a TextSpan and add it to spans.
    fn flush_text(&mut self) {
        if !self.text.is_empty() {
            self.spans.push(TextSpan {
                text: std::mem::take(&mut self.text),
                style: self.inline_style.clone(),
                link: self.link.clone(),
                color: self.span_color,
                font_size_em: self.span_font_size_em,
            });
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
    /// CSS stylesheet for class-based styling.
    stylesheet: StyleSheet,
}

/// Token sink that builds `Vec<ContentBlock>` from XHTML tokens.
struct ContentSink {
    state: RefCell<SinkState>,
}

impl ContentSink {
    fn new(base_href: &str, stylesheet: StyleSheet) -> Self {
        ContentSink {
            state: RefCell::new(SinkState {
                stack: vec![StackEntry::new("root")],
                output: Vec::new(),
                skip_depth: 0,
                base_href: base_href.to_string(),
                pending_images: Vec::new(),
                image_counter: 0,
                stylesheet,
            }),
        }
    }

    fn into_blocks_and_pending(self) -> (Vec<ContentBlock>, Vec<(usize, PendingImage)>) {
        let mut state = self.state.into_inner();
        if let Some(mut root) = state.stack.pop() {
            root.flush_text();
            state.output.extend(root.children);
        }
        (state.output, state.pending_images)
    }
}

const SKIP_TAGS: &[&str] = &["head", "style", "script", "title"];
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];
const INLINE_STYLE_TAGS: &[&str] = &[
    "em", "strong", "b", "i", "u", "del", "s", "code", "ins", "cite", "dfn", "var", "kbd", "samp",
    "tt",
];
const TRANSPARENT_TAGS: &[&str] = &[
    "div",
    "section",
    "article",
    "body",
    "html",
    "small",
    "sub",
    "sup",
    "mark",
    "abbr",
    "nav",
    "main",
    "header",
    "footer",
    "figure",
    "figcaption",
    "details",
    "summary",
    "dl",
    "dt",
    "dd",
    "svg",
];

/// Compute the inline style for a given tag, inheriting from a parent style.
fn style_for_tag(tag: &str, parent: &InlineStyle) -> InlineStyle {
    let mut style = parent.clone();
    match tag {
        "strong" | "b" => style.bold = true,
        "em" | "i" | "cite" | "dfn" | "var" => style.italic = true,
        "u" | "ins" => style.underline = true,
        "del" | "s" => style.strikethrough = true,
        "code" | "kbd" | "samp" | "tt" => style.monospaced = true,
        _ => {}
    }
    style
}

/// Returns true when an `<aside>` element carries `epub:type="footnote"` (or "rearnote").
fn is_footnote_aside(attrs: &[html5ever::Attribute]) -> bool {
    find_attr(attrs, "epub:type").is_some_and(|v| {
        v.split_whitespace()
            .any(|t| matches!(t, "footnote" | "rearnote"))
    })
}

/// Returns true when an element is a container of multiple footnote items.
/// Matches `epub:type="endnotes"`, `role="doc-endnotes"`, and `class` containing "footnote"/"endnote".
fn is_footnote_container_element(attrs: &[html5ever::Attribute]) -> bool {
    let epub_type = find_attr(attrs, "epub:type").unwrap_or_default();
    let role = find_attr(attrs, "role").unwrap_or_default();
    let class = find_attr(attrs, "class").unwrap_or_default();
    epub_type
        .split_whitespace()
        .any(|t| matches!(t, "endnotes" | "rearnotes" | "footnotes"))
        || matches!(role.as_str(), "doc-endnotes" | "doc-footnotes")
        || class
            .split_whitespace()
            .any(|c| c == "footnotes" || c == "endnotes" || c == "footnote")
}

/// Returns true when any ancestor stack entry is a footnote container.
fn in_footnote_container(stack: &[StackEntry]) -> bool {
    stack.iter().any(|e| e.is_footnote_container)
}

/// Collect plain text from spans.
fn plain_text_from_spans(spans: &[TextSpan]) -> String {
    spans.iter().map(|s| s.text.as_str()).collect()
}

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
                        let media_type = guess_media_type(&resolved);

                        // Insert a placeholder block and record it for later resolution
                        let id = state.image_counter;
                        state.image_counter += 1;

                        let placeholder = if media_type == "image/svg+xml" {
                            ContentBlock::Svg {
                                alt: alt.clone(),
                                content: String::new(), // Will be loaded during resolution
                                style: BlockStyle::default(),
                            }
                        } else {
                            ContentBlock::Image {
                                alt: alt.clone(),
                                data: Vec::new(),
                                media_type,
                            }
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
                    } else if !alt.is_empty()
                        && let Some(entry) = state.stack.last_mut()
                    {
                        entry.children.push(ContentBlock::Paragraph {
                            text: format!("[{alt}]"),
                            spans: vec![TextSpan {
                                text: format!("[{alt}]"),
                                style: InlineStyle::default(),
                                link: None,
                                color: None,
                                font_size_em: None,
                            }],
                            style: BlockStyle::default(),
                        });
                    }
                    return TokenSinkResult::Continue;
                }

                // Handle <svg> tags - start accumulating SVG content
                if tag_name == "svg" {
                    let mut entry = StackEntry::new(tag_name);
                    entry.element_id = find_attr(attrs, "id");

                    // Resolve stylesheet styles from class, then merge inline style= on top
                    let class_attr = find_attr(attrs, "class").unwrap_or_default();
                    let css_resolved = state.stylesheet.resolve(tag_name, &class_attr);
                    entry.block_style = css_resolved.block;
                    if let Some(style_attr) = find_attr(attrs, "style") {
                        let inline = parse_inline_style(&style_attr).block;
                        entry.block_style = entry.block_style.merge(inline);
                    }

                    state.stack.push(entry);
                    return TokenSinkResult::Continue;
                }

                // Handle <a href="..."> — flush parent, push entry with inherited style + link
                if tag_name == "a" {
                    if let Some(parent) = state.stack.last_mut() {
                        parent.flush_text();
                    }
                    let parent_style = state
                        .stack
                        .last()
                        .map(|e| e.inline_style.clone())
                        .unwrap_or_default();
                    let parent_link = state.stack.last().and_then(|e| e.link.clone());
                    let href = find_attr(attrs, "href");
                    // Own href takes priority; fall back to inherited link context
                    let link = href.or(parent_link);
                    let mut span_color = None;
                    let mut span_font_size_em = None;
                    if let Some(style_attr) = find_attr(attrs, "style") {
                        let parsed = parse_inline_style(&style_attr);
                        span_color = parsed.color;
                        span_font_size_em = parsed.font_size_em;
                    }
                    let mut entry = StackEntry::new_with_style(tag_name, parent_style, link);
                    entry.span_color = span_color;
                    entry.span_font_size_em = span_font_size_em;
                    state.stack.push(entry);
                    return TokenSinkResult::Continue;
                }

                // Handle inline style tags: flush parent text, push styled entry
                if INLINE_STYLE_TAGS.contains(&tag_name) {
                    // Flush any accumulated text on the parent before entering the styled scope
                    if let Some(parent) = state.stack.last_mut() {
                        parent.flush_text();
                    }
                    let parent_style = state
                        .stack
                        .last()
                        .map(|e| e.inline_style.clone())
                        .unwrap_or_default();
                    // Inherit the enclosing link context (e.g. <a><strong>bold link</strong></a>)
                    let parent_link = state.stack.last().and_then(|e| e.link.clone());
                    let mut style = style_for_tag(tag_name, &parent_style);
                    // Apply stylesheet styles from class
                    let class_attr = find_attr(attrs, "class").unwrap_or_default();
                    let css_resolved = state.stylesheet.resolve(tag_name, &class_attr);
                    style.bold |= css_resolved.inline.bold;
                    style.italic |= css_resolved.inline.italic;
                    style.underline |= css_resolved.inline.underline;
                    style.strikethrough |= css_resolved.inline.strikethrough;
                    style.monospaced |= css_resolved.inline.monospaced;
                    let mut span_color = css_resolved.color;
                    let mut span_font_size_em = css_resolved.font_size_em;
                    // Inline style= overrides stylesheet
                    if let Some(style_attr) = find_attr(attrs, "style") {
                        let parsed = parse_inline_style(&style_attr);
                        style.bold |= parsed.inline.bold;
                        style.italic |= parsed.inline.italic;
                        style.underline |= parsed.inline.underline;
                        style.strikethrough |= parsed.inline.strikethrough;
                        style.monospaced |= parsed.inline.monospaced;
                        if parsed.color.is_some() {
                            span_color = parsed.color;
                        }
                        if parsed.font_size_em.is_some() {
                            span_font_size_em = parsed.font_size_em;
                        }
                    }
                    let mut entry = StackEntry::new_with_style(tag_name, style, parent_link);
                    entry.span_color = span_color;
                    entry.span_font_size_em = span_font_size_em;
                    state.stack.push(entry);
                    return TokenSinkResult::Continue;
                }

                // Handle <span> with style= or class-based styling as an inline-style element
                if tag_name == "span" {
                    let style_attr = find_attr(attrs, "style");
                    let class_attr = find_attr(attrs, "class").unwrap_or_default();
                    let css_resolved = state.stylesheet.resolve("span", &class_attr);
                    let has_style = style_attr.is_some();
                    let has_css = css_resolved.inline != InlineStyle::default()
                        || css_resolved.color.is_some()
                        || css_resolved.font_size_em.is_some();
                    if has_style || has_css {
                        if let Some(parent) = state.stack.last_mut() {
                            parent.flush_text();
                        }
                        let parent_style = state
                            .stack
                            .last()
                            .map(|e| e.inline_style.clone())
                            .unwrap_or_default();
                        let parent_link = state.stack.last().and_then(|e| e.link.clone());
                        // Start with parent, merge CSS, then merge inline style= (inline wins)
                        let mut style = parent_style;
                        style.bold |= css_resolved.inline.bold;
                        style.italic |= css_resolved.inline.italic;
                        style.underline |= css_resolved.inline.underline;
                        style.strikethrough |= css_resolved.inline.strikethrough;
                        style.monospaced |= css_resolved.inline.monospaced;
                        let mut span_color = css_resolved.color;
                        let mut span_font_size_em = css_resolved.font_size_em;
                        if let Some(ref sa) = style_attr {
                            let parsed = parse_inline_style(sa);
                            style.bold |= parsed.inline.bold;
                            style.italic |= parsed.inline.italic;
                            style.underline |= parsed.inline.underline;
                            style.strikethrough |= parsed.inline.strikethrough;
                            style.monospaced |= parsed.inline.monospaced;
                            if parsed.color.is_some() {
                                span_color = parsed.color;
                            }
                            if parsed.font_size_em.is_some() {
                                span_font_size_em = parsed.font_size_em;
                            }
                        }
                        let mut entry = StackEntry::new_with_style("span", style, parent_link);
                        entry.span_color = span_color;
                        entry.span_font_size_em = span_font_size_em;
                        state.stack.push(entry);
                        return TokenSinkResult::Continue;
                    }
                }

                let mut entry = StackEntry::new(tag_name);

                entry.element_id = find_attr(attrs, "id");

                // Resolve stylesheet styles from class, then merge inline style= on top
                let class_attr = find_attr(attrs, "class").unwrap_or_default();
                let css_resolved = state.stylesheet.resolve(tag_name, &class_attr);
                entry.block_style = css_resolved.block;
                if let Some(style_attr) = find_attr(attrs, "style") {
                    let inline = parse_inline_style(&style_attr).block;
                    entry.block_style = entry.block_style.merge(inline);
                }

                if tag_name == "ol"
                    && let Some(start_str) = find_attr(attrs, "start")
                {
                    entry.ol_start = start_str.parse().unwrap_or(1);
                }

                if tag_name == "aside" {
                    entry.is_footnote = is_footnote_aside(attrs);
                } else if matches!(tag_name, "section" | "div" | "ol" | "ul") {
                    entry.is_footnote_container = is_footnote_container_element(attrs);
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

                // Handle inline style, link, and span tag end: promote spans to parent
                if INLINE_STYLE_TAGS.contains(&tag_name) || tag_name == "a" || tag_name == "span" {
                    let mut entry = entry;
                    entry.flush_text();
                    if let Some(parent) = state.stack.last_mut() {
                        // Flush parent's pending text so span ordering is preserved.
                        // For inline-style/styled-span tags the parent was already
                        // flushed at start-tag time (this is a no-op); for unstyled
                        // <span> (generic start path) this is necessary.
                        parent.flush_text();
                        parent.spans.extend(entry.spans);
                    }
                    return TokenSinkResult::Continue;
                }

                let mut entry = entry;
                entry.flush_text();

                // Skip whitespace trimming inside preformatted blocks.
                // After popping the entry, check if the tag itself is <pre>
                // or if a <pre> ancestor remains on the stack.
                let in_pre = tag_name == "pre" || in_preformatted(&state.stack);
                let (text, spans) = if in_pre {
                    let text = plain_text_from_spans(&entry.spans);
                    (text, entry.spans)
                } else {
                    let text = plain_text_from_spans(&entry.spans).trim().to_string();
                    let spans = if text.is_empty() {
                        Vec::new()
                    } else {
                        trim_spans(entry.spans)
                    };
                    (text, spans)
                };

                let block_style = entry.block_style.clone();
                let block = match tag_name {
                    "h1" => Some(ContentBlock::Heading {
                        level: 1,
                        text,
                        spans,
                        style: block_style,
                    }),
                    "h2" => Some(ContentBlock::Heading {
                        level: 2,
                        text,
                        spans,
                        style: block_style,
                    }),
                    "h3" => Some(ContentBlock::Heading {
                        level: 3,
                        text,
                        spans,
                        style: block_style,
                    }),
                    "h4" => Some(ContentBlock::Heading {
                        level: 4,
                        text,
                        spans,
                        style: block_style,
                    }),
                    "h5" => Some(ContentBlock::Heading {
                        level: 5,
                        text,
                        spans,
                        style: block_style,
                    }),
                    "h6" => Some(ContentBlock::Heading {
                        level: 6,
                        text,
                        spans,
                        style: block_style,
                    }),
                    "p" => {
                        if text.is_empty() && entry.children.is_empty() {
                            None
                        } else if entry.children.is_empty() {
                            Some(ContentBlock::Paragraph {
                                text,
                                spans,
                                style: block_style,
                            })
                        } else {
                            let mut blocks = Vec::new();
                            if !text.is_empty() {
                                blocks.push(ContentBlock::Paragraph {
                                    text,
                                    spans,
                                    style: block_style,
                                });
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
                    "pre" => {
                        if tag_name == "pre" || !state.stack.iter().any(|e| e.tag == "pre") {
                            if !entry.text.is_empty() || !spans.is_empty() {
                                let raw_text = if spans.is_empty() {
                                    entry.text
                                } else {
                                    plain_text_from_spans(&spans)
                                };
                                Some(ContentBlock::Preformatted {
                                    text: raw_text,
                                    spans,
                                    style: block_style,
                                })
                            } else {
                                None
                            }
                        } else {
                            if let Some(parent) = state.stack.last_mut() {
                                parent.text.push_str(&text);
                            }
                            None
                        }
                    }
                    "blockquote" => {
                        let mut children = entry.children;
                        if !text.is_empty() {
                            children.insert(
                                0,
                                ContentBlock::Paragraph {
                                    text,
                                    spans,
                                    style: BlockStyle::default(),
                                },
                            );
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
                        } else if !entry.children.is_empty() {
                            // Footnote blocks collected from <li> elements inside a footnote container
                            promote_to_parent(
                                &mut state,
                                String::new(),
                                Vec::new(),
                                entry.children,
                                Vec::new(),
                            );
                            None
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
                        } else if !entry.children.is_empty() {
                            // Footnote blocks collected from <li> elements inside a footnote container
                            promote_to_parent(
                                &mut state,
                                String::new(),
                                Vec::new(),
                                entry.children,
                                Vec::new(),
                            );
                            None
                        } else {
                            None
                        }
                    }
                    "li" => {
                        if in_footnote_container(&state.stack) {
                            let id = entry.element_id.unwrap_or_default();
                            let mut blocks = entry.children;
                            if !text.is_empty() || !spans.is_empty() {
                                blocks.insert(
                                    0,
                                    ContentBlock::Paragraph {
                                        text,
                                        spans,
                                        style: block_style,
                                    },
                                );
                            }
                            if let Some(parent) = state.stack.last_mut() {
                                parent.children.push(ContentBlock::Footnote { id, blocks });
                            }
                        } else if let Some(parent) = state.stack.last_mut() {
                            parent.list_items.push(ListItem {
                                text,
                                spans,
                                style: block_style,
                            });
                        }
                        None
                    }
                    "aside" => {
                        if entry.is_footnote {
                            let id = entry.element_id.unwrap_or_default();
                            let mut blocks = entry.children;
                            if !text.is_empty() || !spans.is_empty() {
                                blocks.insert(
                                    0,
                                    ContentBlock::Paragraph {
                                        text,
                                        spans,
                                        style: BlockStyle::default(),
                                    },
                                );
                            }
                            if !blocks.is_empty() {
                                Some(ContentBlock::Footnote { id, blocks })
                            } else {
                                None
                            }
                        } else {
                            promote_to_parent(
                                &mut state,
                                text,
                                spans,
                                entry.children,
                                entry.list_items,
                            );
                            None
                        }
                    }
                    "td" | "th" => {
                        if let Some(parent) = state.stack.last_mut() {
                            let is_header = tag_name == "th";
                            // Prefer inline content (spans/text); fall back to block children
                            let (cell_text, cell_spans) = if !spans.is_empty() || !text.is_empty() {
                                (text, spans)
                            } else {
                                // Collect text/spans from block-level children (e.g. <p> in <td>)
                                let mut t = String::new();
                                let mut s: Vec<TextSpan> = Vec::new();
                                for child in &entry.children {
                                    let (ct, cs) = match child {
                                        ContentBlock::Paragraph { text, spans, .. } => {
                                            (text.as_str(), spans.as_slice())
                                        }
                                        ContentBlock::Heading { text, spans, .. } => {
                                            (text.as_str(), spans.as_slice())
                                        }
                                        _ => continue,
                                    };
                                    if !t.is_empty() {
                                        t.push(' ');
                                    }
                                    t.push_str(ct);
                                    s.extend_from_slice(cs);
                                }
                                (t, s)
                            };
                            parent.table_cells.push(TableCell {
                                text: cell_text,
                                spans: cell_spans,
                                is_header,
                            });
                        }
                        None
                    }
                    "tr" => {
                        if let Some(parent) = state.stack.last_mut()
                            && !entry.table_cells.is_empty()
                        {
                            parent.table_rows.push(entry.table_cells);
                        }
                        None
                    }
                    "thead" | "tbody" | "tfoot" => {
                        if let Some(parent) = state.stack.last_mut() {
                            parent.table_rows.extend(entry.table_rows);
                        }
                        None
                    }
                    "table" => {
                        if !entry.table_rows.is_empty() {
                            Some(ContentBlock::Table {
                                rows: entry.table_rows,
                            })
                        } else {
                            None
                        }
                    }
                    "svg" => {
                        if !entry.svg_content.is_empty() {
                            Some(ContentBlock::Svg {
                                alt: String::new(), // SVG elements don't have alt text
                                content: entry.svg_content,
                                style: block_style,
                            })
                        } else {
                            None
                        }
                    }
                    _ if TRANSPARENT_TAGS.contains(&tag_name) => {
                        promote_to_parent(
                            &mut state,
                            text,
                            spans,
                            entry.children,
                            entry.list_items,
                        );
                        None
                    }
                    _ => {
                        promote_to_parent(
                            &mut state,
                            text,
                            spans,
                            entry.children,
                            entry.list_items,
                        );
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
                let in_pre = in_preformatted(&state.stack);
                if let Some(entry) = state.stack.last_mut() {
                    // Accumulate raw content for SVG elements
                    if entry.tag == "svg" {
                        entry.svg_content.push_str(&text);
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
                        let normalized = normalize_html_whitespace(&text, prev_ends_with_space);
                        entry.text.push_str(&normalized);
                    }
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
    spans: Vec<TextSpan>,
    children: Vec<ContentBlock>,
    list_items: Vec<ListItem>,
) {
    if let Some(parent) = state.stack.last_mut() {
        if !spans.is_empty() {
            // Child had styled (or any) content captured as spans.
            // Flush the parent's own pending plain text into a span first, then
            // append the child's spans. Do NOT also push `text` — it is the
            // plain-text projection of `spans` and would cause duplication.
            parent.flush_text();
            parent.spans.extend(spans);
        } else if !text.is_empty() {
            // Child had only unstyled plain text (no spans produced).
            // Append it to the parent's accumulating text buffer.
            if !parent.text.is_empty() && !parent.text.ends_with(char::is_whitespace) {
                parent.text.push(' ');
            }
            parent.text.push_str(&text);
        }
        parent.children.extend(children);
        parent.list_items.extend(list_items);
    } else {
        if !text.is_empty() {
            state.output.push(ContentBlock::Paragraph {
                text,
                spans,
                style: BlockStyle::default(),
            });
        }
        state.output.extend(children);
    }
}

/// Returns true when any ancestor stack entry (or the current one) is a `<pre>` element.
fn in_preformatted(stack: &[StackEntry]) -> bool {
    stack.iter().any(|e| e.tag == "pre")
}

/// Normalize a run of HTML character data per the HTML whitespace rules:
/// collapse any sequence of ASCII whitespace (space, tab, CR, LF) to a single
/// space.  If the already-accumulated text for this element ends with a space
/// the leading space of the normalized result is suppressed to avoid doubling.
fn normalize_html_whitespace(text: &str, prev_ends_with_space: bool) -> String {
    let mut result = String::with_capacity(text.len());
    let mut in_ws = false;
    for ch in text.chars() {
        if matches!(ch, ' ' | '\t' | '\r' | '\n') {
            if !in_ws {
                in_ws = true;
                result.push(' ');
            }
        } else {
            in_ws = false;
            result.push(ch);
        }
    }
    // Drop the leading space if the parent text already ends with one.
    if prev_ends_with_space && let Some(stripped) = result.strip_prefix(' ') {
        return stripped.to_string();
    }
    result
}

/// Result of parsing a `style="..."` attribute or CSS declaration block,
/// containing both block-level and inline-level properties.
pub(super) struct ParsedStyle {
    pub(super) block: BlockStyle,
    pub(super) inline: InlineStyle,
    /// Per-span color override (from `color:` property).
    pub(super) color: Option<[u8; 3]>,
    /// Per-span font-size multiplier (from `font-size:` property).
    pub(super) font_size_em: Option<f32>,
}

/// Parse CSS declarations (from a `style="..."` attribute or a CSS rule body)
/// into block-level and inline-level properties.
///
/// Exposed as `pub(super)` so the stylesheet module can reuse the same property parsing.
pub(super) fn parse_css_declarations(style_attr: &str) -> ParsedStyle {
    parse_inline_style(style_attr)
}

/// Parse a `style="..."` attribute value into block-level and inline-level properties.
/// Only a small subset of CSS properties is recognised; unknown properties are ignored.
fn parse_inline_style(style_attr: &str) -> ParsedStyle {
    let mut result = ParsedStyle {
        block: BlockStyle::default(),
        inline: InlineStyle::default(),
        color: None,
        font_size_em: None,
    };
    for declaration in style_attr.split(';') {
        let declaration = declaration.trim();
        if declaration.is_empty() {
            continue;
        }
        let Some((prop, value)) = declaration.split_once(':') else {
            continue;
        };
        let prop = prop.trim().to_ascii_lowercase();
        let value = value.trim();
        match prop.as_str() {
            "text-align" => {
                result.block.text_align = match value.to_ascii_lowercase().as_str() {
                    "center" => Some(TextAlign::Center),
                    "right" => Some(TextAlign::Right),
                    "left" => Some(TextAlign::Left),
                    _ => None,
                };
            }
            "font-size" => {
                let em = parse_css_length_as_em(value);
                result.block.font_size_em = em;
                result.font_size_em = em;
            }
            "color" => {
                let c = parse_css_color(value);
                result.block.color = c;
                result.color = c;
            }
            "margin-top" => {
                result.block.margin_top_em = parse_css_length_as_em(value);
            }
            "margin-bottom" => {
                result.block.margin_bottom_em = parse_css_length_as_em(value);
            }
            "font-weight" => {
                let v = value.to_ascii_lowercase();
                if v == "bold" || v == "bolder" {
                    result.inline.bold = true;
                } else if let Ok(n) = v.parse::<u32>()
                    && n >= 700
                {
                    result.inline.bold = true;
                }
            }
            "font-style" => {
                let v = value.to_ascii_lowercase();
                if v == "italic" || v == "oblique" {
                    result.inline.italic = true;
                }
            }
            "text-decoration" => {
                let v = value.to_ascii_lowercase();
                if v.contains("underline") {
                    result.inline.underline = true;
                }
                if v.contains("line-through") {
                    result.inline.strikethrough = true;
                }
            }
            "font-family" => {
                let v = value.to_ascii_lowercase();
                if v.contains("monospace") || v.contains("courier") || v.contains("consolas") {
                    result.inline.monospaced = true;
                }
            }
            _ => {}
        }
    }
    result
}

/// Parse a CSS length value into an `em` multiplier.
/// Supported units: `em`, `px` (converted as px/16), `%` (divided by 100).
fn parse_css_length_as_em(value: &str) -> Option<f32> {
    let v = value.trim().to_ascii_lowercase();
    if let Some(n) = v.strip_suffix("em") {
        n.trim().parse().ok()
    } else if let Some(n) = v.strip_suffix("px") {
        n.trim().parse::<f32>().ok().map(|px| px / 16.0)
    } else if let Some(n) = v.strip_suffix('%') {
        n.trim().parse::<f32>().ok().map(|pct| pct / 100.0)
    } else {
        None
    }
}

/// Parse a CSS color value into `[r, g, b]`.
/// Supports `#rrggbb`, `#rgb`, and `rgb(r, g, b)`.
fn parse_css_color(value: &str) -> Option<[u8; 3]> {
    let v = value.trim();
    if let Some(hex) = v.strip_prefix('#') {
        match hex.len() {
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some([r, g, b])
            }
            3 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
                Some([r, g, b])
            }
            _ => None,
        }
    } else if let Some(inner) = v
        .to_ascii_lowercase()
        .strip_prefix("rgb(")
        .and_then(|s| s.strip_suffix(')'))
    {
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() == 3 {
            let r = parts[0].trim().parse().ok()?;
            let g = parts[1].trim().parse().ok()?;
            let b = parts[2].trim().parse().ok()?;
            Some([r, g, b])
        } else {
            None
        }
    } else {
        None
    }
}

fn find_attr(attrs: &[html5ever::Attribute], name: &str) -> Option<String> {
    let target = html5ever::LocalName::from(name);
    attrs
        .iter()
        .find(|a| a.name.local == target)
        .map(|a| a.value.to_string())
}

/// Trim leading and trailing whitespace from a span list, preserving interior whitespace.
fn trim_spans(mut spans: Vec<TextSpan>) -> Vec<TextSpan> {
    // Trim leading whitespace on first span
    if let Some(first) = spans.first_mut() {
        let trimmed = first.text.trim_start().to_string();
        if trimmed.is_empty() {
            spans.remove(0);
            // Recurse in case next span also has leading whitespace
            return trim_spans(spans);
        }
        first.text = trimmed;
    }
    // Trim trailing whitespace on last span
    if let Some(last) = spans.last_mut() {
        let trimmed = last.text.trim_end().to_string();
        if trimmed.is_empty() {
            spans.pop();
            return trim_spans(spans);
        }
        last.text = trimmed;
    }
    spans
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

    let (mut blocks, pending_images) = tokenizer.sink.into_blocks_and_pending();

    // Resolve pending images by walking the block tree
    if !pending_images.is_empty() {
        resolve_images(&mut blocks, &pending_images, resolve_image);
    }

    blocks
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
            } if data.is_empty() => {
                // This is a placeholder — find matching pending image
                if let Some((_, img)) = pending.iter().find(|(_, img)| img.alt == *alt) {
                    if let Some((resolved_data, resolved_mt)) = resolve_image(&img.resolved_path) {
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
            ContentBlock::Svg { alt, content, .. } if content.is_empty() => {
                // This is a placeholder SVG from img tag - find matching pending image
                if let Some((_, img)) = pending.iter().find(|(_, img)| img.alt == *alt) {
                    if let Some((resolved_data, resolved_mt)) = resolve_image(&img.resolved_path) {
                        if resolved_mt == "image/svg+xml" {
                            *content = String::from_utf8_lossy(&resolved_data).into_owned();
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
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
