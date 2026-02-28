#[derive(Clone, Debug)]
pub struct NavEntry {
    /// Full href including fragment, e.g. `"EPUB/Text/ch1.xhtml#section2"`.
    pub href: String,
    pub label: String,
    /// Nesting depth within the TOC (0 = top-level).
    pub depth: usize,
}
