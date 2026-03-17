/// Classification of an HTML tag for parsing purposes.
pub(super) enum TagClass {
    /// head, style, script, title — content skipped entirely.
    Skip,
    /// Void elements that never have a closing tag (br, hr, img, …).
    Void,
    /// Inline formatting tags (em, strong, b, i, u, …).
    InlineStyle,
    /// Structural containers whose `id` attribute is promoted to an Anchor
    /// block (section, article, div, nav, main, header, footer).
    /// These are also transparent (children are promoted to the parent).
    AnchorContainer,
    /// Transparent containers whose children are promoted to the parent
    /// but which do *not* emit an Anchor block (body, html, small, …).
    Transparent,
    /// Any tag not covered by the above categories.
    Other,
}

/// Classify an HTML tag name for the content parser.
pub(super) fn classify(tag: &str) -> TagClass {
    match tag {
        "head" | "style" | "script" | "title" => TagClass::Skip,
        "area" | "base" | "br" | "col" | "embed" | "hr" | "img" | "input" | "link" | "meta"
        | "param" | "source" | "track" | "wbr" => TagClass::Void,
        "em" | "strong" | "b" | "i" | "u" | "del" | "s" | "code" | "ins" | "cite" | "dfn"
        | "var" | "kbd" | "samp" | "tt" => TagClass::InlineStyle,
        "section" | "article" | "div" | "nav" | "main" | "header" | "footer" => {
            TagClass::AnchorContainer
        }
        "body" | "html" | "small" | "sub" | "sup" | "mark" | "abbr" | "details" | "summary"
        | "dl" | "dt" | "dd" | "svg" => TagClass::Transparent,
        _ => TagClass::Other,
    }
}
