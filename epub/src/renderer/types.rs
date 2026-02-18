use serde::Deserialize;
use serde::Serialize;

use crate::domain::locator::Locator;

/// Declares what a renderer implementation supports.
#[derive(Clone, Debug)]
pub struct RendererCapabilities {
    pub supports_dom_navigation: bool,
    pub supports_selection: bool,
    pub supports_pagination: bool,
    pub supports_text_extraction: bool,
}

/// A range of visible text identified by start/end locators.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextRange {
    pub start: Locator,
    pub end: Locator,
}

/// A rendered frame produced by a renderer for display.
/// Wraps raw pixel data with dimensions.
#[derive(Clone, Debug)]
pub struct RenderFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}
