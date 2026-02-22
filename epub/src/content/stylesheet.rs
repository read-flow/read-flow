use super::block::BlockStyle;
use super::block::InlineStyle;
use super::block::TextAlign;
use super::parser::parse_css_declarations;

/// A simple CSS selector — only tag, class, and tag.class are supported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CssSelector {
    /// Matches a tag name, e.g. `p`, `h1`, `div`.
    Tag(String),
    /// Matches a class name, e.g. `.verse` (stored without the dot).
    Class(String),
    /// Matches a tag with a class, e.g. `p.indent`.
    TagAndClass(String, String),
}

/// Resolved style properties from a stylesheet rule.
#[derive(Clone, Debug, Default)]
pub struct ResolvedStyle {
    pub block: BlockStyle,
    pub inline: InlineStyle,
    pub color: Option<[u8; 3]>,
    pub font_size_em: Option<f32>,
}

impl ResolvedStyle {
    /// Merge another resolved style on top of this one (other wins on conflicts).
    fn merge_from(&mut self, other: &ResolvedStyle) {
        if other.block.text_align.is_some() {
            self.block.text_align = other.block.text_align.clone();
        }
        if other.block.font_size_em.is_some() {
            self.block.font_size_em = other.block.font_size_em;
        }
        if other.block.color.is_some() {
            self.block.color = other.block.color;
        }
        if other.block.margin_top_em.is_some() {
            self.block.margin_top_em = other.block.margin_top_em;
        }
        if other.block.margin_bottom_em.is_some() {
            self.block.margin_bottom_em = other.block.margin_bottom_em;
        }
        self.inline.bold |= other.inline.bold;
        self.inline.italic |= other.inline.italic;
        self.inline.underline |= other.inline.underline;
        self.inline.strikethrough |= other.inline.strikethrough;
        self.inline.monospaced |= other.inline.monospaced;
        if other.color.is_some() {
            self.color = other.color;
        }
        if other.font_size_em.is_some() {
            self.font_size_em = other.font_size_em;
        }
    }
}

/// A parsed CSS stylesheet containing simple rules.
#[derive(Clone, Debug)]
pub struct StyleSheet {
    rules: Vec<(CssSelector, ResolvedStyle)>,
}

impl StyleSheet {
    /// An empty stylesheet that matches nothing.
    pub fn empty() -> Self {
        StyleSheet { rules: Vec::new() }
    }

    /// Resolve the applicable style for a given tag name and class attribute value.
    ///
    /// Walks rules in source order; later rules override earlier ones.
    /// The class attribute may contain multiple space-separated class names.
    pub fn resolve(&self, tag: &str, class_attr: &str) -> ResolvedStyle {
        let classes: Vec<&str> = class_attr.split_whitespace().collect();
        let mut result = ResolvedStyle::default();

        for (selector, style) in &self.rules {
            let matches = match selector {
                CssSelector::Tag(t) => t == tag,
                CssSelector::Class(c) => classes.contains(&c.as_str()),
                CssSelector::TagAndClass(t, c) => t == tag && classes.contains(&c.as_str()),
            };
            if matches {
                result.merge_from(style);
            }
        }

        result
    }

    /// Merge another stylesheet into this one (appended rules win over existing).
    pub fn merge(&mut self, other: StyleSheet) {
        self.rules.extend(other.rules);
    }
}

/// Parse a CSS text into a `StyleSheet`.
///
/// Strips comments and `@`-rules, then parses simple selectors (tag, `.class`, `tag.class`).
/// Combinators, pseudo-classes, IDs, and attribute selectors are ignored.
pub fn parse_css(text: &str) -> StyleSheet {
    let stripped = strip_css_comments(text);
    let stripped = strip_at_rules(&stripped);
    parse_rules(&stripped)
}

/// Parse rule blocks from preprocessed CSS text.
fn parse_rules(text: &str) -> StyleSheet {
    let mut rules = Vec::new();

    for chunk in text.split('}') {
        let chunk = chunk.trim();
        if chunk.is_empty() {
            continue;
        }
        let Some((selectors_str, declarations_str)) = chunk.split_once('{') else {
            continue;
        };

        let parsed = parse_css_declarations(declarations_str.trim());
        let mut style = ResolvedStyle {
            block: parsed.block,
            inline: parsed.inline,
            color: parsed.color,
            font_size_em: parsed.font_size_em,
        };

        // Handle justify -> Left fallback (iced doesn't support justify)
        if style.block.text_align.is_none() {
            let lower = declarations_str.to_ascii_lowercase();
            if lower.contains("text-align") && lower.contains("justify") {
                style.block.text_align = Some(TextAlign::Left);
            }
        }

        for selector_text in selectors_str.split(',') {
            let selector_text = selector_text.trim();
            if selector_text.is_empty() {
                continue;
            }
            if let Some(selector) = parse_selector(selector_text) {
                rules.push((selector, style.clone()));
            }
        }
    }

    StyleSheet { rules }
}

/// Parse a single simple CSS selector.
/// Returns `None` for selectors we don't support (combinators, pseudo-classes, IDs, etc.).
fn parse_selector(s: &str) -> Option<CssSelector> {
    let s = s.trim();
    // Reject selectors with combinators or pseudo-classes
    if s.contains(' ') || s.contains('>') || s.contains('+') || s.contains('~') || s.contains(':') {
        return None;
    }
    // Reject ID selectors
    if s.contains('#') {
        return None;
    }
    // Reject attribute selectors
    if s.contains('[') {
        return None;
    }

    if let Some(dot_pos) = s.find('.') {
        let tag_part = &s[..dot_pos];
        let class_part = &s[dot_pos + 1..];
        if class_part.is_empty() {
            return None;
        }
        // Reject chained classes like `.a.b`
        if class_part.contains('.') {
            return None;
        }
        if tag_part.is_empty() {
            Some(CssSelector::Class(class_part.to_ascii_lowercase()))
        } else {
            Some(CssSelector::TagAndClass(
                tag_part.to_ascii_lowercase(),
                class_part.to_ascii_lowercase(),
            ))
        }
    } else {
        // Plain tag selector
        Some(CssSelector::Tag(s.to_ascii_lowercase()))
    }
}

/// Strip `/* ... */` comments from CSS text.
fn strip_css_comments(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.char_indices().peekable();

    while let Some((_, ch)) = chars.next() {
        if ch == '/' && chars.peek().is_some_and(|(_, c)| *c == '*') {
            chars.next(); // consume '*'
            while let Some((_, c)) = chars.next() {
                if c == '*' && chars.peek().is_some_and(|(_, c2)| *c2 == '/') {
                    chars.next(); // consume '/'
                    break;
                }
            }
            continue;
        }
        result.push(ch);
    }

    result
}

/// Strip `@`-rules (e.g. `@media`, `@font-face`, `@import`) from CSS text.
fn strip_at_rules(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '@' {
            let mut depth = 0;
            let mut found_brace = false;
            for c in chars.by_ref() {
                if c == '{' {
                    depth += 1;
                    found_brace = true;
                } else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                } else if c == ';' && !found_brace {
                    break;
                }
            }
        } else {
            result.push(ch);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tag_selector() {
        let sheet = parse_css("p { text-align: center; }");
        assert_eq!(sheet.rules.len(), 1);
        assert_eq!(sheet.rules[0].0, CssSelector::Tag("p".into()));
        assert_eq!(sheet.rules[0].1.block.text_align, Some(TextAlign::Center));
    }

    #[test]
    fn parse_class_selector() {
        let sheet = parse_css(".verse { font-style: italic; }");
        assert_eq!(sheet.rules.len(), 1);
        assert_eq!(sheet.rules[0].0, CssSelector::Class("verse".into()));
        assert!(sheet.rules[0].1.inline.italic);
    }

    #[test]
    fn parse_tag_and_class_selector() {
        let sheet = parse_css("p.indent { margin-top: 0.5em; }");
        assert_eq!(sheet.rules.len(), 1);
        assert_eq!(
            sheet.rules[0].0,
            CssSelector::TagAndClass("p".into(), "indent".into())
        );
        assert_eq!(sheet.rules[0].1.block.margin_top_em, Some(0.5));
    }

    #[test]
    fn parse_justify_fallback_to_left() {
        let sheet = parse_css("p { text-align: justify; }");
        assert_eq!(sheet.rules.len(), 1);
        assert_eq!(sheet.rules[0].1.block.text_align, Some(TextAlign::Left));
    }

    #[test]
    fn parse_multiple_selectors_comma_separated() {
        let sheet = parse_css("h1, h2, h3 { text-align: center; }");
        assert_eq!(sheet.rules.len(), 3);
        assert_eq!(sheet.rules[0].0, CssSelector::Tag("h1".into()));
        assert_eq!(sheet.rules[1].0, CssSelector::Tag("h2".into()));
        assert_eq!(sheet.rules[2].0, CssSelector::Tag("h3".into()));
    }

    #[test]
    fn parse_multiple_rules() {
        let sheet = parse_css(
            "p { text-align: justify; }
             .verse { font-style: italic; }
             h1 { text-align: center; font-size: 2em; }",
        );
        assert_eq!(sheet.rules.len(), 3);
    }

    #[test]
    fn comments_stripped() {
        let sheet = parse_css("/* heading styles */ h1 { text-align: center; } /* end */");
        assert_eq!(sheet.rules.len(), 1);
        assert_eq!(sheet.rules[0].0, CssSelector::Tag("h1".into()));
    }

    #[test]
    fn combinator_selectors_ignored() {
        let sheet = parse_css("div p { color: red; }");
        assert_eq!(
            sheet.rules.len(),
            0,
            "descendant combinator should be ignored"
        );
    }

    #[test]
    fn pseudo_class_selectors_ignored() {
        let sheet = parse_css("a:hover { color: red; }");
        assert_eq!(sheet.rules.len(), 0, "pseudo-class should be ignored");
    }

    #[test]
    fn id_selectors_ignored() {
        let sheet = parse_css("#main { color: red; }");
        assert_eq!(sheet.rules.len(), 0, "ID selector should be ignored");
    }

    #[test]
    fn resolve_tag_match() {
        let sheet = parse_css("p { text-align: center; }");
        let resolved = sheet.resolve("p", "");
        assert_eq!(resolved.block.text_align, Some(TextAlign::Center));
    }

    #[test]
    fn resolve_class_match() {
        let sheet = parse_css(".verse { text-align: center; }");
        let resolved = sheet.resolve("p", "verse");
        assert_eq!(resolved.block.text_align, Some(TextAlign::Center));
    }

    #[test]
    fn resolve_tag_and_class_match() {
        let sheet = parse_css("p.indent { margin-top: 0.5em; }");
        let resolved = sheet.resolve("p", "indent");
        assert_eq!(resolved.block.margin_top_em, Some(0.5));
        // Should not match other tags
        let resolved = sheet.resolve("div", "indent");
        assert_eq!(resolved.block.margin_top_em, None);
    }

    #[test]
    fn resolve_multiple_classes() {
        let sheet = parse_css(
            ".bold { font-weight: bold; }
             .italic { font-style: italic; }",
        );
        let resolved = sheet.resolve("span", "bold italic");
        assert!(resolved.inline.bold);
        assert!(resolved.inline.italic);
    }

    #[test]
    fn resolve_no_match() {
        let sheet = parse_css("h1 { text-align: center; }");
        let resolved = sheet.resolve("p", "");
        assert_eq!(resolved.block.text_align, None);
    }

    #[test]
    fn resolve_later_rule_wins() {
        let sheet = parse_css(
            "p { text-align: left; }
             p { text-align: center; }",
        );
        let resolved = sheet.resolve("p", "");
        assert_eq!(resolved.block.text_align, Some(TextAlign::Center));
    }

    #[test]
    fn empty_stylesheet() {
        let sheet = StyleSheet::empty();
        let resolved = sheet.resolve("p", "verse");
        assert_eq!(resolved.block, BlockStyle::default());
    }

    #[test]
    fn merge_stylesheets() {
        let mut a = parse_css("p { text-align: left; }");
        let b = parse_css(".verse { text-align: center; }");
        a.merge(b);
        assert_eq!(a.rules.len(), 2);
        let resolved = a.resolve("p", "verse");
        assert_eq!(resolved.block.text_align, Some(TextAlign::Center));
    }

    #[test]
    fn at_rules_stripped() {
        let sheet = parse_css(
            "@import url('other.css');
             @font-face { font-family: 'Custom'; src: url('font.woff'); }
             p { text-align: center; }",
        );
        assert_eq!(sheet.rules.len(), 1);
        assert_eq!(sheet.rules[0].0, CssSelector::Tag("p".into()));
    }

    #[test]
    fn css_color_in_stylesheet() {
        let sheet = parse_css(".red { color: #ff0000; }");
        let resolved = sheet.resolve("span", "red");
        assert_eq!(resolved.color, Some([255, 0, 0]));
        assert_eq!(resolved.block.color, Some([255, 0, 0]));
    }

    #[test]
    fn css_font_size_in_stylesheet() {
        let sheet = parse_css(".big { font-size: 1.5em; }");
        let resolved = sheet.resolve("span", "big");
        assert_eq!(resolved.font_size_em, Some(1.5));
        assert_eq!(resolved.block.font_size_em, Some(1.5));
    }
}
