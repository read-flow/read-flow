//! Block heights measured by laying out the elements the blocks are drawn
//! with, so pagination and scroll positions agree with what the reader sees.

use cosmic::Element;
use cosmic::iced::Rectangle;
use cosmic::iced::Size;
use cosmic::iced::advanced::widget::Id;
use cosmic::iced::advanced::widget::Operation;
use cosmic::iced::core::renderer::Headless;
use cosmic::iced::runtime::user_interface::Cache;
use cosmic::iced::runtime::user_interface::UserInterface;
use cosmic::widget;
use epub::ContentBlock;

use crate::render::BlockHighlight;
use crate::render::RenderContext;
use crate::render::RenderMessage;

/// A renderer that lays out elements without a window.
pub fn headless_renderer() -> cosmic::Renderer {
    futures::executor::block_on(<cosmic::Renderer as Headless>::new(
        cosmic::font::default(),
        cosmic::iced::Pixels(16.0),
        Some("tiny-skia"),
    ))
    .expect("create headless renderer")
}

/// Lay out `element` in `bounds` with `renderer` and run `operation` over the
/// laid-out widgets.
pub fn lay_out<Message>(
    renderer: &mut cosmic::Renderer,
    element: Element<'_, Message>,
    bounds: Size,
    operation: &mut dyn Operation<()>,
) {
    let mut ui = UserInterface::build(element, bounds, Cache::default(), renderer);
    ui.operate(renderer, operation);
}

/// Measures the height elements are laid out with.
#[derive(Default)]
pub struct BlockMeasurer {
    /// Created on first use, so a viewer that never measures needs none.
    renderer: Option<cosmic::Renderer>,
}

impl BlockMeasurer {
    /// Height `element` is laid out with when it is `width` pixels wide and
    /// may grow as tall as it needs.
    ///
    /// An element with a zero fixed width or height measures 0: a column
    /// leaves it out entirely, without spacing around it.
    pub fn height(&mut self, element: Element<'_, RenderMessage>, width: f32) -> f32 {
        if element.as_widget().size_hint().is_void() {
            return 0.0;
        }

        struct RootBounds(Option<Rectangle>);

        impl Operation<()> for RootBounds {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<()>)) {
                if self.0.is_none() {
                    operate(self);
                }
            }

            fn container(&mut self, _id: Option<&Id>, bounds: Rectangle) {
                self.0.get_or_insert(bounds);
            }
        }

        let renderer = self.renderer.get_or_insert_with(headless_renderer);
        let mut root = RootBounds(None);
        lay_out(
            renderer,
            widget::container(element).width(width).into(),
            Size::new(width, f32::INFINITY),
            &mut root,
        );
        root.0.map_or(0.0, |bounds| bounds.height)
    }
}

/// Height of `block` drawn by `ctx` `width` pixels wide.
pub fn block_height(
    measurer: &mut BlockMeasurer,
    ctx: &RenderContext<'_>,
    block: &ContentBlock,
    width: f32,
) -> f32 {
    measurer.height(ctx.render_block(block, BlockHighlight::None), width)
}
