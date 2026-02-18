use super::types::RenderFrame;
use super::types::RendererCapabilities;
use crate::domain::locator::Locator;

/// Commands sent to an async/hosted renderer (EPUB.md section 22.2).
#[derive(Clone, Debug)]
pub enum RendererCommand {
    LoadSpine(usize),
    GoTo(Locator),
    GetLocator,
    Resize { width: u32, height: u32 },
    Shutdown,
}

/// Events emitted by an async/hosted renderer (EPUB.md section 22.2).
#[derive(Clone, Debug)]
pub enum RendererEvent {
    LocatorChanged(Locator),
    FrameReady(RenderFrame),
    Capabilities(RendererCapabilities),
}
