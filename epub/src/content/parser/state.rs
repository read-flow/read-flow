use std::cell::RefCell;

use super::util::PendingImage;
use super::util::plain_text_from_spans;
use crate::content::block::BlockStyle;
use crate::content::block::ContentBlock;
use crate::content::block::InlineStyle;
use crate::content::block::ListItem;
use crate::content::block::TableCell;
use crate::content::block::TextSpan;
use crate::content::stylesheet::StyleSheet;

/// An element on the parsing stack.
pub(super) struct StackEntry {
    pub(super) tag: String,
    pub(super) text: String,
    pub(super) children: Vec<ContentBlock>,
    pub(super) list_items: Vec<ListItem>,
    /// For `<ol>` elements, the start attribute value.
    pub(super) ol_start: u32,
    /// Accumulated styled spans for this element.
    pub(super) spans: Vec<TextSpan>,
    /// The inherited inline style at this stack level.
    pub(super) inline_style: InlineStyle,
    /// Href from an enclosing `<a>` element, inherited by child entries.
    pub(super) link: Option<String>,
    /// Accumulated rows for `<table>` elements; each row is a `Vec<TableCell>`.
    pub(super) table_rows: Vec<Vec<TableCell>>,
    /// Accumulated cells for the current `<tr>` element.
    pub(super) table_cells: Vec<TableCell>,
    /// The `id` HTML attribute of this element, used for footnote anchoring.
    pub(super) element_id: Option<String>,
    /// True when this is an `<aside epub:type="footnote">` element.
    pub(super) is_footnote: bool,
    /// True when this element is a container of footnote items
    /// (`class="footnotes"`, `role="doc-endnotes"`, `epub:type="endnotes"`, etc.).
    pub(super) is_footnote_container: bool,
    /// Block-level style parsed from the element's `style="..."` attribute.
    pub(super) block_style: BlockStyle,
    /// Per-span color override from a `style="color:..."` attribute on an inline element.
    pub(super) span_color: Option<[u8; 3]>,
    /// Per-span font-size multiplier from a `style="font-size:..."` attribute on an inline element.
    pub(super) span_font_size_em: Option<f32>,
    /// Accumulated SVG XML content for `<svg>` elements.
    pub(super) svg_content: String,
    /// Aspect ratio (width/height) from the `viewBox` attribute of the `<svg>` element.
    pub(super) svg_aspect_ratio: Option<f32>,
    /// Caption spans from a `<figcaption>` child (populated when this entry is a `<figure>`).
    pub(super) figure_caption_spans: Vec<TextSpan>,
    /// Plain-text caption from a `<figcaption>` child.
    pub(super) figure_caption_text: String,
}

impl StackEntry {
    pub(super) fn new(tag: &str) -> Self {
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
            svg_aspect_ratio: None,
            figure_caption_spans: Vec::new(),
            figure_caption_text: String::new(),
        }
    }

    pub(super) fn new_with_style(tag: &str, style: InlineStyle, link: Option<String>) -> Self {
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
            svg_aspect_ratio: None,
            figure_caption_spans: Vec::new(),
            figure_caption_text: String::new(),
        }
    }

    /// Flush any accumulated text into a TextSpan and add it to spans.
    pub(super) fn flush_text(&mut self) {
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

/// State accumulated during tokenization.
pub(super) struct SinkState {
    pub(super) stack: Vec<StackEntry>,
    pub(super) output: Vec<ContentBlock>,
    pub(super) skip_depth: usize,
    pub(super) base_href: String,
    /// Images collected during parsing, keyed to their position in the output.
    /// Each entry is `(block_index_path, pending_image)` where block_index_path
    /// identifies where in the output tree the placeholder lives.
    pub(super) pending_images: Vec<(usize, PendingImage)>,
    /// Counter for placeholder images inserted into the output.
    pub(super) image_counter: usize,
    /// CSS stylesheet for class-based styling.
    pub(super) stylesheet: StyleSheet,
}

/// Token sink that builds `Vec<ContentBlock>` from XHTML tokens.
pub(super) struct ContentSink {
    pub(super) state: RefCell<SinkState>,
}

impl ContentSink {
    pub(super) fn new(base_href: &str, stylesheet: StyleSheet) -> Self {
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

    pub(super) fn into_blocks_and_pending(self) -> (Vec<ContentBlock>, Vec<(usize, PendingImage)>) {
        let mut state = self.state.into_inner();
        if let Some(mut root) = state.stack.pop() {
            root.flush_text();
            state.output.extend(root.children);
            // Convert dangling root-level spans (e.g. from a standalone <a> directly
            // in <body>) into a trailing paragraph so they are not silently discarded.
            if !root.spans.is_empty() {
                let text = plain_text_from_spans(&root.spans).trim().to_string();
                if !text.is_empty() {
                    state.output.push(ContentBlock::Paragraph {
                        text,
                        spans: root.spans,
                        style: BlockStyle::default(),
                    });
                }
            }
        }
        (state.output, state.pending_images)
    }
}

/// Returns true when any ancestor stack entry (or the current one) is a `<pre>` element.
pub(super) fn in_preformatted(stack: &[StackEntry]) -> bool {
    stack.iter().any(|e| e.tag == "pre")
}

/// Returns true when any ancestor stack entry is a footnote container.
pub(super) fn in_footnote_container(stack: &[StackEntry]) -> bool {
    stack.iter().any(|e| e.is_footnote_container)
}
