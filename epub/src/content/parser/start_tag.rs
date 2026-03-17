use super::classify::TagClass;
use super::classify::classify;
use super::state::SinkState;
use super::state::StackEntry;
use super::util::PendingImage;
use super::util::find_attr;
use super::util::is_footnote_aside;
use super::util::is_footnote_container_element;
use super::util::parse_inline_style;
use super::util::parse_viewbox_str;
use super::util::style_for_tag;
use crate::content::block::BlockStyle;
use crate::content::block::ContentBlock;
use crate::content::block::InlineStyle;
use crate::content::block::TextSpan;
use crate::content::resolve::base_dir;
use crate::content::resolve::guess_media_type;
use crate::content::resolve::resolve_href;

/// Process an HTML start tag, updating `state` accordingly.
///
/// Returns `true` if the token was fully handled and the caller should move on
/// to the next token.  Returns `false` only for the generic "push a block-level
/// entry" path where processing falls through to the normal stack push.
pub(super) fn handle_start_tag(
    state: &mut SinkState,
    tag_name: &str,
    attrs: &[html5ever::Attribute],
    self_closing: bool,
) {
    if state.skip_depth > 0 {
        if !self_closing && !matches!(classify(tag_name), TagClass::Void) {
            state.skip_depth += 1;
        }
        return;
    }

    if matches!(classify(tag_name), TagClass::Skip) {
        state.skip_depth = 1;
        return;
    }

    if tag_name == "br" {
        if let Some(entry) = state.stack.last_mut() {
            entry.text.push('\n');
        }
        return;
    }

    if tag_name == "hr" {
        if let Some(entry) = state.stack.last_mut() {
            entry.children.push(ContentBlock::HorizontalRule);
        }
        return;
    }

    if tag_name == "img" {
        handle_img(state, attrs);
        return;
    }

    // Handle <svg> tags - start accumulating SVG content
    if tag_name == "svg" {
        handle_svg_start(state, tag_name, attrs, self_closing);
        return;
    }

    // Capture tags within SVG elements
    if let Some(svg_entry) = state.stack.iter_mut().find(|e| e.tag == "svg") {
        svg_entry.svg_content.push('<');
        svg_entry.svg_content.push_str(tag_name);
        for attr in attrs {
            svg_entry.svg_content.push(' ');
            svg_entry.svg_content.push_str(attr.name.local.as_ref());
            svg_entry.svg_content.push_str("=\"");
            svg_entry.svg_content.push_str(&attr.value);
            svg_entry.svg_content.push('"');
        }
        if self_closing {
            svg_entry.svg_content.push_str("/>");
        } else {
            svg_entry.svg_content.push('>');
        }
    }

    // Handle <a href="..."> — flush parent, push entry with inherited style + link
    if tag_name == "a" {
        handle_anchor(state, attrs);
        return;
    }

    // Handle inline style tags: flush parent text, push styled entry
    if matches!(classify(tag_name), TagClass::InlineStyle) {
        handle_inline_tag(state, tag_name, attrs);
        return;
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
            return;
        }
    }

    // Generic block-level element: build a new stack entry
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

fn handle_img(state: &mut SinkState, attrs: &[html5ever::Attribute]) {
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
                aspect_ratio: None, // Set during resolution
            }
        } else {
            ContentBlock::Image {
                alt: alt.clone(),
                data: Vec::new(),
                media_type,
                natural_width: 0, // Set during resolution
                natural_height: 0,
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
}

fn handle_svg_start(
    state: &mut SinkState,
    tag_name: &str,
    attrs: &[html5ever::Attribute],
    self_closing: bool,
) {
    let mut entry = StackEntry::new(tag_name);
    entry.element_id = find_attr(attrs, "id");
    entry.svg_aspect_ratio = find_attr(attrs, "viewbox")
        .as_deref()
        .and_then(parse_viewbox_str);

    // Resolve stylesheet styles from class, then merge inline style= on top
    let class_attr = find_attr(attrs, "class").unwrap_or_default();
    let css_resolved = state.stylesheet.resolve(tag_name, &class_attr);
    entry.block_style = css_resolved.block;
    if let Some(style_attr) = find_attr(attrs, "style") {
        let inline = parse_inline_style(&style_attr).block;
        entry.block_style = entry.block_style.merge(inline);
    }

    // Start accumulating the SVG markup
    entry.svg_content.push('<');
    entry.svg_content.push_str(tag_name);
    for attr in attrs {
        entry.svg_content.push(' ');
        entry.svg_content.push_str(attr.name.local.as_ref());
        entry.svg_content.push_str("=\"");
        entry.svg_content.push_str(&attr.value);
        entry.svg_content.push('"');
    }
    if self_closing {
        entry.svg_content.push_str("/>");
    } else {
        entry.svg_content.push('>');
    }

    state.stack.push(entry);
}

fn handle_anchor(state: &mut SinkState, attrs: &[html5ever::Attribute]) {
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
    let mut entry = StackEntry::new_with_style("a", parent_style, link);
    entry.span_color = span_color;
    entry.span_font_size_em = span_font_size_em;
    state.stack.push(entry);
}

fn handle_inline_tag(state: &mut SinkState, tag_name: &str, attrs: &[html5ever::Attribute]) {
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
}
