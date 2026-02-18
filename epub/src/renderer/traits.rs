use super::types::RenderFrame;
use super::types::RendererCapabilities;
use super::types::TextRange;
use crate::domain::document::Document;
use crate::domain::locator::Locator;
use crate::domain::spine::SpineItem;

/// Core renderer interface (EPUB.md section 17).
///
/// Implementors render EPUB content and track reading position.
/// Concrete implementations may wrap a WebKit subprocess, a native
/// Rust layout engine, or a headless extractor.
pub trait Renderer {
    fn capabilities(&self) -> RendererCapabilities;
    fn load_document(&mut self, doc: &dyn Document);
    fn load_spine_item(&mut self, item: &SpineItem);
    fn go_to(&mut self, locator: &Locator);
    fn current_locator(&self) -> Locator;
    fn visible_text_range(&self) -> Option<TextRange>;
    fn shutdown(&mut self);
}

/// Visual surface abstraction (EPUB.md section 18).
///
/// Receives rendered frames and presents them to the user.
pub trait RenderSurface {
    fn resize(&mut self, width: u32, height: u32);
    fn present(&mut self, frame: RenderFrame);
}
