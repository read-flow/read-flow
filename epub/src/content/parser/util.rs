use crate::content::block::BlockStyle;
use crate::content::block::InlineStyle;
use crate::content::block::TextAlign;
use crate::content::block::TextSpan;

/// A pending image to be resolved after parsing.
pub(super) struct PendingImage {
    /// Resolved zip path for the image.
    pub(super) resolved_path: String,
    /// Alt text from the `<img>` element.
    pub(super) alt: String,
}

/// Look up an attribute by (local) name in an html5ever attribute list.
pub(super) fn find_attr(attrs: &[html5ever::Attribute], name: &str) -> Option<String> {
    let target = html5ever::LocalName::from(name);
    attrs
        .iter()
        .find(|a| a.name.local == target)
        .map(|a| a.value.to_string())
}

/// Normalize a run of HTML character data per the HTML whitespace rules:
/// collapse any sequence of ASCII whitespace (space, tab, CR, LF) to a single
/// space.  If the already-accumulated text for this element ends with a space
/// the leading space of the normalized result is suppressed to avoid doubling.
pub(super) fn normalize_html_whitespace(text: &str, prev_ends_with_space: bool) -> String {
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

/// Trim leading and trailing whitespace from a span list, preserving interior whitespace.
pub(super) fn trim_spans(mut spans: Vec<TextSpan>) -> Vec<TextSpan> {
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

/// Collect plain text from a span list.
pub(super) fn plain_text_from_spans(spans: &[TextSpan]) -> String {
    spans.iter().map(|s| s.text.as_str()).collect()
}

/// Compute the inline style for a given tag, inheriting from a parent style.
pub(super) fn style_for_tag(tag: &str, parent: &InlineStyle) -> InlineStyle {
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
pub(super) fn is_footnote_aside(attrs: &[html5ever::Attribute]) -> bool {
    find_attr(attrs, "epub:type").is_some_and(|v| {
        v.split_whitespace()
            .any(|t| matches!(t, "footnote" | "rearnote"))
    })
}

/// Returns true when an element is a container of multiple footnote items.
/// Matches `epub:type="endnotes"`, `role="doc-endnotes"`, and `class` containing "footnote"/"endnote".
pub(super) fn is_footnote_container_element(attrs: &[html5ever::Attribute]) -> bool {
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

/// Parse the width-to-height ratio from a `viewBox="min-x min-y w h"` attribute value.
pub(super) fn parse_viewbox_str(viewbox: &str) -> Option<f32> {
    let mut parts = viewbox.split_whitespace();
    let _min_x: f32 = parts.next()?.parse().ok()?;
    let _min_y: f32 = parts.next()?.parse().ok()?;
    let w: f32 = parts.next()?.parse().ok()?;
    let h: f32 = parts.next()?.parse().ok()?;
    if h > 0.0 { Some(w / h) } else { None }
}

/// Result of parsing a `style="..."` attribute or CSS declaration block,
/// containing both block-level and inline-level properties.
pub(crate) struct ParsedStyle {
    pub(crate) block: BlockStyle,
    pub(crate) inline: InlineStyle,
    /// Per-span color override (from `color:` property).
    pub(crate) color: Option<[u8; 3]>,
    /// Per-span font-size multiplier (from `font-size:` property).
    pub(crate) font_size_em: Option<f32>,
}

/// Parse CSS declarations (from a `style="..."` attribute or a CSS rule body)
/// into block-level and inline-level properties.
///
/// Exposed so the stylesheet module can reuse the same property parsing.
pub(crate) fn parse_css_declarations(style_attr: &str) -> ParsedStyle {
    parse_inline_style(style_attr)
}

/// Parse a `style="..."` attribute value into block-level and inline-level properties.
/// Only a small subset of CSS properties is recognised; unknown properties are ignored.
pub(super) fn parse_inline_style(style_attr: &str) -> ParsedStyle {
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
pub(super) fn parse_css_length_as_em(value: &str) -> Option<f32> {
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
pub(super) fn parse_css_color(value: &str) -> Option<[u8; 3]> {
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
