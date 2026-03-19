use cosmic::Element;
use cosmic::Renderer;
use cosmic::Theme;
use cosmic::iced_core::Font;
use cosmic::iced_core::Pixels;
use cosmic::iced_core::Size;
use cosmic::iced_core::mouse;
use cosmic::iced_core::renderer;
use cosmic::iced_core::renderer::Headless;
use cosmic::iced_core::theme;
use cosmic::iced_runtime::UserInterface;
use cosmic::iced_runtime::user_interface;

/// A headless renderer that draws cosmic widgets to an in-memory RGBA buffer.
pub struct HeadlessRenderer {
    renderer: Renderer,
    theme: Theme,
}

impl HeadlessRenderer {
    /// Creates a new headless renderer using the tiny-skia software backend and the light theme.
    pub fn new() -> Self {
        let renderer = futures::executor::block_on(<Renderer as Headless>::new(
            Font::DEFAULT,
            Pixels(16.0),
            Some("tiny-skia"),
        ))
        .expect("create tiny-skia headless renderer");

        Self {
            renderer,
            theme: Theme::light(),
        }
    }

    /// Creates a new headless renderer with the given theme.
    pub fn with_theme(theme: Theme) -> Self {
        let mut r = Self::new();
        r.theme = theme;
        r
    }

    /// Renders `element` into a pixel buffer of the given size.
    ///
    /// Returns raw RGBA bytes (4 bytes per pixel, row-major).
    pub fn render<Message>(
        &mut self,
        element: Element<'_, Message>,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        let logical = Size::new(width as f32, height as f32);

        let mut ui = UserInterface::build(
            element,
            logical,
            user_interface::Cache::default(),
            &mut self.renderer,
        );

        let base = theme::Base::base(&self.theme);

        ui.draw(
            &mut self.renderer,
            &self.theme,
            &renderer::Style {
                icon_color: base.text_color,
                text_color: base.text_color,
                scale_factor: 1.0,
            },
            mouse::Cursor::Unavailable,
        );

        self.renderer
            .screenshot(Size { width, height }, 1.0, base.background_color)
    }
}

impl Default for HeadlessRenderer {
    fn default() -> Self {
        Self::new()
    }
}
