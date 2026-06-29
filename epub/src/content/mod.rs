mod block;
mod parser;
mod resolve;
mod stylesheet;

pub use block::BlockStyle;
pub use block::ContentBlock;
pub use block::InlineStyle;
pub use block::ListItem;
pub use block::TableCell;
pub use block::TextAlign;
pub use block::TextSpan;
pub use parser::parse_xhtml;
pub use parser::parse_xhtml_with_paths;
pub use parser::resolve_svg_images;
pub use resolve::base_dir;
pub use resolve::guess_media_type;
pub use resolve::resolve_href;
pub use stylesheet::StyleSheet;
pub use stylesheet::parse_css;

/// Parse an HTML fragment (e.g. from an OPDS `<content type="html">` element)
/// into content blocks. The fragment is wrapped in a minimal XHTML document
/// before parsing; common void HTML elements (`<br>`, `<hr>`) are normalised
/// to self-closing form for XHTML compatibility.
pub fn parse_html_fragment(html: &str) -> Vec<ContentBlock> {
    let normalized = html
        .replace("<br>", "<br/>")
        .replace("<BR>", "<br/>")
        .replace("<hr>", "<hr/>")
        .replace("<HR>", "<hr/>");
    let doc = format!(
        r#"<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml"><body>{normalized}</body></html>"#
    );
    parse_xhtml(doc.as_bytes(), "", &StyleSheet::empty(), &mut |_| None)
}
