use super::classify::TagClass;
use super::classify::classify;
use super::state::SinkState;
use super::state::StackEntry;
use super::state::in_footnote_container;
use super::state::in_preformatted;
use super::util::plain_text_from_spans;
use super::util::trim_spans;
use crate::content::block::BlockStyle;
use crate::content::block::ContentBlock;
use crate::content::block::ListItem;
use crate::content::block::TableCell;
use crate::content::block::TextSpan;

/// Compute the DOM node path for the element represented by `entry`, relative
/// to the content document root (i.e. starting from `<html>`'s children).
///
/// `state.stack` must already have `entry` popped.  The path is built from
/// the element-child indices stored on each remaining stack entry, skipping
/// the pseudo-root (index 0) and the `<html>` entry (index 1, since CFI paths
/// after `!` start from html's children), then appending `entry`'s own index.
fn block_path(stack: &[StackEntry], entry_index: u32) -> Vec<u32> {
    stack
        .iter()
        .skip(2) // skip pseudo-root and <html>
        .map(|e| e.element_child_index)
        .chain(std::iter::once(entry_index))
        .collect()
}

/// Push `block` and its corresponding `path` into the parent or top-level output.
fn push_block(state: &mut SinkState, block: ContentBlock, path: Vec<u32>) {
    if let Some(parent) = state.stack.last_mut() {
        parent.children_paths.push(path);
        parent.children.push(block);
    } else {
        state.output_block_paths.push(path);
        state.output.push(block);
    }
}

/// Process an HTML end tag, updating `state` accordingly.
///
/// The entry for `tag_name` has already been popped from the stack and
/// is passed in as `entry`.
pub(super) fn handle_end_tag(state: &mut SinkState, tag_name: &str, entry: StackEntry) {
    // Compute the DOM path for this element once, reused for all blocks it emits.
    let path = block_path(&state.stack, entry.element_child_index);
    // Capture end tags within SVG elements (but not the SVG closing tag itself)
    if tag_name != "svg"
        && let Some(svg_entry) = state.stack.iter_mut().find(|e| e.tag == "svg")
    {
        svg_entry.svg_content.push_str("</");
        svg_entry.svg_content.push_str(tag_name);
        svg_entry.svg_content.push('>');
    }

    // Handle inline style, link, and span tag end: promote spans to parent
    if matches!(classify(tag_name), TagClass::InlineStyle) || tag_name == "a" || tag_name == "span"
    {
        let mut entry = entry;
        entry.flush_text();
        // Fragment-only links (e.g. href="#fn1") are footnote call-site references.
        // Wrap their visible text in square brackets so they read as "[1]" instead
        // of plain "1", making them visually distinct from surrounding prose.
        if tag_name == "a"
            && entry.link.as_deref().is_some_and(|l| l.starts_with('#'))
            && !entry.spans.is_empty()
        {
            entry.spans.first_mut().unwrap().text.insert(0, '[');
            entry.spans.last_mut().unwrap().text.push(']');
        }
        if let Some(parent) = state.stack.last_mut() {
            // Flush parent's pending text so span ordering is preserved.
            // For inline-style/styled-span tags the parent was already
            // flushed at start-tag time (this is a no-op); for unstyled
            // <span> (generic start path) this is necessary.
            parent.flush_text();
            parent.spans.extend(entry.spans);
        }
        return;
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
    // Save element_id for Anchor emission (must be extracted before
    // entry fields are moved into match arms or promote_to_parent).
    let element_id = entry.element_id.clone();

    let block = match tag_name {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            let level = tag_name[1..].parse::<u8>().unwrap();
            Some(ContentBlock::Heading {
                level,
                text,
                spans,
                style: block_style,
            })
        }
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
                // Mixed content: text + block children (e.g. <p>text<img/>more</p>).
                // Build parallel block/path lists so children_paths stays in sync.
                let mut blocks = Vec::new();
                let mut paths: Vec<Vec<u32>> = Vec::new();
                if !text.is_empty() {
                    blocks.push(ContentBlock::Paragraph {
                        text,
                        spans,
                        style: block_style,
                    });
                    paths.push(path.clone());
                }
                let n = entry.children.len();
                blocks.extend(entry.children);
                let mut cpaths = entry.children_paths;
                cpaths.resize(n, vec![]);
                paths.extend(cpaths);
                paths.resize(blocks.len(), vec![]);
                if let Some(parent) = state.stack.last_mut() {
                    parent.children.extend(blocks);
                    parent.children_paths.extend(paths);
                } else {
                    state.output.extend(blocks);
                    state.output_block_paths.extend(paths);
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
                    state,
                    path.clone(),
                    String::new(),
                    Vec::new(),
                    entry.children,
                    entry.children_paths,
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
                    state,
                    path.clone(),
                    String::new(),
                    Vec::new(),
                    entry.children,
                    entry.children_paths,
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
        "aside" if entry.is_footnote => {
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
            // Check if SVG has actual content beyond just the opening tag
            let has_content = entry.svg_content.len() > entry.tag.len() + 2; // > "<svg>"
            if has_content {
                let mut svg_content = entry.svg_content;
                svg_content.push_str("</svg>");
                Some(ContentBlock::Svg {
                    alt: String::new(), // SVG elements don't have alt text
                    content: svg_content,
                    style: block_style,
                    aspect_ratio: entry.svg_aspect_ratio,
                })
            } else {
                None
            }
        }
        "figcaption" => {
            // Store caption on the parent <figure> entry.
            // If not inside a <figure> (malformed HTML), fall back
            // to rendering it as a plain paragraph.
            if state.stack.last().is_some_and(|e| e.tag == "figure") {
                if let Some(parent) = state.stack.last_mut() {
                    parent.figure_caption_spans = spans;
                    parent.figure_caption_text = text;
                }
            } else if !text.is_empty() || !spans.is_empty() {
                let block = ContentBlock::Paragraph {
                    text,
                    spans,
                    style: block_style,
                };
                push_block(state, block, path.clone());
            }
            None
        }
        "figure" => {
            let mut blocks = entry.children;
            // Any text directly in <figure> (outside figcaption) becomes a paragraph.
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
            let caption = entry.figure_caption_spans;
            let caption_text = entry.figure_caption_text;
            if !blocks.is_empty() || !caption.is_empty() || !caption_text.is_empty() {
                Some(ContentBlock::Figure {
                    blocks,
                    caption,
                    caption_text,
                })
            } else {
                None
            }
        }
        _ if matches!(
            classify(tag_name),
            TagClass::AnchorContainer | TagClass::Transparent
        ) =>
        {
            // If this container has CSS block styling AND only direct text/spans
            // (no block-level children), emit a styled paragraph so the style is
            // preserved (e.g. `div.part { text-align: center; font-size: 0.7em; }`).
            // The element-id Anchor is emitted by the general code below.
            if block_style != BlockStyle::default()
                && entry.children.is_empty()
                && entry.list_items.is_empty()
                && (!text.is_empty() || !spans.is_empty())
            {
                Some(ContentBlock::Paragraph {
                    text,
                    spans,
                    style: block_style,
                })
            } else {
                // Emit an Anchor before promoted children for structural
                // container elements that carry a non-empty id.
                if let Some(id) = &element_id
                    && !id.is_empty()
                    && matches!(classify(tag_name), TagClass::AnchorContainer)
                {
                    let anchor = ContentBlock::Anchor { id: id.clone() };
                    push_block(state, anchor, path.clone());
                }
                promote_to_parent(
                    state,
                    path.clone(),
                    text,
                    spans,
                    entry.children,
                    entry.children_paths,
                    entry.list_items,
                );
                None
            }
        }
        _ => {
            promote_to_parent(
                state,
                path.clone(),
                text,
                spans,
                entry.children,
                entry.children_paths,
                entry.list_items,
            );
            None
        }
    };

    // Flush any anchor IDs collected from inline <a id="..."> elements that
    // appeared inside this block.  Emit them before the block itself so that
    // anchor_y_from_heights maps the id to this block's y position, enabling
    // footnote back-reference links (e.g. href="#fnref1") to navigate directly
    // to the call-site paragraph.
    if block.is_some() {
        // Collect first to avoid a double-borrow of `state` inside the loop.
        let inline_anchors: Vec<String> = state.pending_inline_anchors.drain(..).collect();
        for id in inline_anchors {
            push_block(state, ContentBlock::Anchor { id }, vec![]);
        }
    }

    // For non-transparent blocks (headings, paragraphs, etc.),
    // emit an Anchor before the block itself when the element had an id.
    // Footnote blocks already carry their own id for anchor lookup, so
    // skip them to avoid duplicating the anchor entry.
    if block.is_some()
        && !matches!(block, Some(ContentBlock::Footnote { .. }))
        && let Some(id) = &element_id
        && !id.is_empty()
    {
        let anchor = ContentBlock::Anchor { id: id.clone() };
        push_block(state, anchor, path.clone());
    }
    if let Some(block) = block {
        push_block(state, block, path);
    }
}

fn promote_to_parent(
    state: &mut SinkState,
    container_path: Vec<u32>,
    text: String,
    spans: Vec<TextSpan>,
    children: Vec<ContentBlock>,
    children_paths: Vec<Vec<u32>>,
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
        let n = children.len();
        parent.children.extend(children);
        let mut cpaths = children_paths;
        cpaths.resize(n, vec![]);
        parent.children_paths.extend(cpaths);
        parent.list_items.extend(list_items);
    } else {
        if !text.is_empty() {
            state.output.push(ContentBlock::Paragraph {
                text,
                spans,
                style: BlockStyle::default(),
            });
            state.output_block_paths.push(container_path);
        }
        let n = children.len();
        state.output.extend(children);
        let mut cpaths = children_paths;
        cpaths.resize(n, vec![]);
        state.output_block_paths.extend(cpaths);
    }
}
