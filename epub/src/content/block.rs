#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InlineStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
}

#[derive(Clone, Debug)]
pub struct TextSpan {
    pub text: String,
    pub style: InlineStyle,
    /// Href from an enclosing `<a>` element, if any.
    pub link: Option<String>,
}

#[derive(Clone, Debug)]
pub enum ContentBlock {
    Heading {
        level: u8,
        text: String,
        spans: Vec<TextSpan>,
    },
    Paragraph {
        text: String,
        spans: Vec<TextSpan>,
    },
    Preformatted {
        text: String,
        spans: Vec<TextSpan>,
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
    Table {
        rows: Vec<Vec<TableCell>>,
    },
    HorizontalRule,
}

#[derive(Clone, Debug)]
pub struct ListItem {
    pub text: String,
    pub spans: Vec<TextSpan>,
}

#[derive(Clone, Debug)]
pub struct TableCell {
    pub text: String,
    pub spans: Vec<TextSpan>,
    pub is_header: bool,
}
