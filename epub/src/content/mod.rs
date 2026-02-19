mod block;
mod parser;
mod resolve;

pub use block::ContentBlock;
pub use block::InlineStyle;
pub use block::ListItem;
pub use block::TextSpan;
pub use parser::parse_xhtml;
pub use resolve::base_dir;
pub use resolve::guess_media_type;
pub use resolve::resolve_href;
