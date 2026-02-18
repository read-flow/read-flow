#[derive(Clone, Debug)]
pub enum ContentBlock {
    Heading {
        level: u8,
        text: String,
    },
    Paragraph {
        text: String,
    },
    Preformatted {
        text: String,
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
    HorizontalRule,
}

#[derive(Clone, Debug)]
pub struct ListItem {
    pub text: String,
}
