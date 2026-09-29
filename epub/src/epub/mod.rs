pub mod container;
pub mod document;
pub mod nav;
pub mod package;

use std::borrow::Cow;

/// Decode an XML document from the archive for parsing.
///
/// quick-xml rejects input that is not valid UTF-8, so invalid byte sequences are replaced
/// with U+FFFD instead; a single bad byte should not make a whole book unreadable.
fn decode_lossy(xml: &[u8]) -> Cow<'_, str> {
    String::from_utf8_lossy(xml)
}
