#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InlineStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub monospaced: bool,
}

#[derive(Clone, Debug)]
pub struct TextSpan {
    pub text: String,
    pub style: InlineStyle,
    /// Href from an enclosing `<a>` element, if any.
    pub link: Option<String>,
    /// Per-span color override from `color: ...` in a `style` attribute.
    pub color: Option<[u8; 3]>,
    /// Per-span font-size multiplier from `font-size: ...` in a `style` attribute.
    pub font_size_em: Option<f32>,
}

/// Horizontal text alignment derived from a `text-align` CSS property.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// Block-level style properties extracted from a `style="..."` attribute or CSS stylesheet.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlockStyle {
    pub text_align: Option<TextAlign>,
    /// `font-size` expressed as an em multiplier (1.0 = normal body size).
    pub font_size_em: Option<f32>,
    /// RGB color from `color: #rrggbb` / `color: rgb(r,g,b)`.
    pub color: Option<[u8; 3]>,
    /// Top margin as em multiplier.
    pub margin_top_em: Option<f32>,
    /// Bottom margin as em multiplier.
    pub margin_bottom_em: Option<f32>,
}

impl BlockStyle {
    /// Merge `other` on top of `self`: values present in `other` override `self`.
    pub fn merge(self, other: BlockStyle) -> BlockStyle {
        BlockStyle {
            text_align: other.text_align.or(self.text_align),
            font_size_em: other.font_size_em.or(self.font_size_em),
            color: other.color.or(self.color),
            margin_top_em: other.margin_top_em.or(self.margin_top_em),
            margin_bottom_em: other.margin_bottom_em.or(self.margin_bottom_em),
        }
    }
}

#[derive(Clone, Debug)]
pub enum ContentBlock {
    Heading {
        level: u8,
        text: String,
        spans: Vec<TextSpan>,
        style: BlockStyle,
    },
    Paragraph {
        text: String,
        spans: Vec<TextSpan>,
        style: BlockStyle,
    },
    Preformatted {
        text: String,
        spans: Vec<TextSpan>,
        style: BlockStyle,
    },
    BlockQuote {
        children: Vec<ContentBlock>,
    },
    UnorderedList {
        items: Vec<ListItem>,
    },
    OrderedList {
        start: u32,
        items: Vec<ListItem>,
    },
    Image {
        alt: String,
        data: Vec<u8>,
        media_type: String,
    },
    Svg {
        alt: String,
        content: String,
        style: BlockStyle,
    },
    Table {
        rows: Vec<Vec<TableCell>>,
    },
    HorizontalRule,
    /// A footnote body, identified by its HTML `id` attribute.
    /// Produced by `<aside epub:type="footnote">` or `<li>` elements inside
    /// a footnote-section container (`class="footnotes"`, `role="doc-endnotes"`, etc.).
    Footnote {
        id: String,
        blocks: Vec<ContentBlock>,
    },
}

#[derive(Clone, Debug)]
pub struct ListItem {
    pub text: String,
    pub spans: Vec<TextSpan>,
    pub style: BlockStyle,
}

#[derive(Clone, Debug)]
pub struct TableCell {
    pub text: String,
    pub spans: Vec<TextSpan>,
    pub is_header: bool,
}
