// SPDX-License-Identifier: AGPL-3.0-or-later

use cosmic::Apply;
use cosmic::Element;
use cosmic::Renderer;
use cosmic::Theme;
use cosmic::iced::Length;
use cosmic::iced::Point;
use cosmic::iced::Rectangle;
use cosmic::iced::Size;
use cosmic::iced::Vector;
use cosmic::iced::advanced::Clipboard;
use cosmic::iced::advanced::Layout;
use cosmic::iced::advanced::Shell;
use cosmic::iced::advanced::Widget;
use cosmic::iced::advanced::layout;
use cosmic::iced::advanced::mouse;
use cosmic::iced::advanced::overlay;
use cosmic::iced::advanced::renderer;
use cosmic::iced::advanced::widget::Tree;
use cosmic::iced::font;
use cosmic::theme;
use cosmic::widget;
use cosmic::widget::button;

use crate::combo_box::DropdownOverlay;
use crate::combo_box::dropdown_container_style;
use crate::combo_box::option_button_style;

/// A font-family picker combining a text input (for filtering) with a floating
/// overlay list where each option is rendered in its own font.
///
/// The text input uses the font of the *confirmed* selection (`selected`), not
/// the current filter query, so it stays stable while the user types.
pub struct FontPicker<'a, Message> {
    options: &'a [&'static str],
    placeholder: String,
    value: &'a str,
    selected: Option<&'static str>,
    on_change: Box<dyn Fn(String) -> Message + 'a>,
    on_select: Option<Box<dyn Fn(String) -> Message + 'a>>,
    on_open: Option<Message>,
    on_close: Option<Message>,
    on_clear: Option<Message>,
    focused: bool,
    width: Length,
}

impl<'a, Message: Clone + 'static> FontPicker<'a, Message> {
    pub fn new(
        options: &'a [&'static str],
        placeholder: impl Into<String>,
        value: &'a str,
        on_change: impl Fn(String) -> Message + 'a,
    ) -> Self {
        Self {
            options,
            placeholder: placeholder.into(),
            value,
            selected: None,
            on_change: Box::new(on_change),
            on_select: None,
            on_open: None,
            on_close: None,
            on_clear: None,
            focused: false,
            width: Length::Fill,
        }
    }

    /// The confirmed font selection — drives the font used to render the text input.
    pub fn selected(mut self, name: &'static str) -> Self {
        self.selected = Some(name);
        self
    }

    /// Callback fired when an option row is clicked (defaults to `on_change`).
    pub fn on_select(mut self, on_select: impl Fn(String) -> Message + 'a) -> Self {
        self.on_select = Some(Box::new(on_select));
        self
    }

    /// Message emitted when the text input gains focus.
    pub fn on_open(mut self, msg: Message) -> Self {
        self.on_open = Some(msg);
        self
    }

    /// Message emitted when the input loses focus or the popup is dismissed.
    pub fn on_close(mut self, msg: Message) -> Self {
        self.on_close = Some(msg);
        self
    }

    /// Message emitted when the clear icon button is pressed.
    pub fn on_clear(mut self, msg: Message) -> Self {
        self.on_clear = Some(msg);
        self
    }

    /// Controls whether the option overlay is currently shown.
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub fn width(mut self, width: Length) -> Self {
        self.width = width;
        self
    }

    pub fn view(self) -> Element<'a, Message> {
        let cosmic::cosmic_theme::Spacing {
            space_xxxs,
            space_xs,
            ..
        } = theme::active().cosmic().spacing;

        let lower = self.value.to_lowercase();
        let visible: Vec<&'static str> = if self.value.is_empty() {
            self.options.to_vec()
        } else {
            self.options
                .iter()
                .copied()
                .filter(|o| o.to_lowercase().contains(&lower))
                .collect()
        };

        let option_messages: Vec<(&'static str, Message)> =
            if let Some(ref on_select) = self.on_select {
                visible
                    .into_iter()
                    .map(|name| (name, on_select(name.to_string())))
                    .collect()
            } else {
                visible
                    .into_iter()
                    .map(|name| (name, (self.on_change)(name.to_string())))
                    .collect()
            };

        let has_overlay = self.focused && !option_messages.is_empty();

        let on_close = self.on_close;
        let on_change = self.on_change;

        let input_font = self
            .selected
            .map(|name| cosmic::iced::Font {
                family: font::Family::Name(name),
                ..cosmic::iced::Font::DEFAULT
            })
            .unwrap_or(cosmic::iced::Font::DEFAULT);

        // When closed, show the confirmed selection; when open, show the filter query.
        let display_value = if self.focused {
            self.value
        } else {
            self.selected.unwrap_or(self.value)
        };

        let mut input = widget::text_input(self.placeholder, display_value)
            .font(input_font)
            .on_input(on_change)
            .width(Length::Fill);

        if let Some(msg) = self.on_open {
            input = input.on_focus(msg);
        }
        if let Some(msg) = on_close.clone() {
            input = input.on_unfocus(msg);
        }
        if let Some(msg) = self.on_clear.filter(|_| !self.value.is_empty()) {
            let clear_icon = widget::icon::from_name("edit-clear-symbolic")
                .size(16)
                .apply(button::custom)
                .class(cosmic::theme::Button::Icon)
                .on_press(msg)
                .padding([0, 4])
                .into();
            input = input.trailing_icon(clear_icon);
        }

        let rows: Vec<Element<'a, Message>> = option_messages
            .into_iter()
            .map(|(name, msg)| {
                let item_font = cosmic::iced::Font {
                    family: font::Family::Name(name),
                    ..cosmic::iced::Font::DEFAULT
                };
                button::custom(
                    widget::text(name)
                        .font(item_font)
                        .apply(widget::container)
                        .padding([space_xxxs, space_xs])
                        .width(Length::Fill),
                )
                .class(option_button_style())
                .width(Length::Fill)
                .on_press(msg)
                .into()
            })
            .collect();

        let dropdown: Element<'a, Message> = widget::container(widget::scrollable::vertical(
            widget::column::with_children(rows),
        ))
        .max_height(400.0)
        .class(dropdown_container_style())
        .width(Length::Fill)
        .into();

        Element::new(FontPickerWidget {
            text_input: input.into(),
            dropdown,
            has_overlay,
            width: self.width,
        })
    }
}

// ─── Inner widget ─────────────────────────────────────────────────────────────

struct FontPickerWidget<'a, Message> {
    text_input: Element<'a, Message>,
    dropdown: Element<'a, Message>,
    has_overlay: bool,
    width: Length,
}

impl<'a, Message: Clone + 'static> Widget<Message, Theme, Renderer>
    for FontPickerWidget<'a, Message>
{
    fn size(&self) -> Size<Length> {
        Size::new(self.width, Length::Shrink)
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let limits = limits.width(self.width);
        self.text_input
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, &limits)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.text_input.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        )
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.text_input), Tree::new(&self.dropdown)]
    }

    fn diff(&mut self, tree: &mut Tree) {
        tree.diff_children(&mut [&mut self.text_input, &mut self.dropdown]);
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.text_input
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &cosmic::iced::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.text_input.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.text_input.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        if !self.has_overlay {
            return self.text_input.as_widget_mut().overlay(
                &mut tree.children[0],
                layout,
                renderer,
                viewport,
                translation,
            );
        }

        let bounds = layout.bounds();
        Some(overlay::Element::new(Box::new(DropdownOverlay {
            content: &mut self.dropdown,
            tree: &mut tree.children[1],
            position: Point::new(bounds.x, bounds.y + bounds.height) + translation,
            width: bounds.width,
        })))
    }
}

#[cfg(test)]
mod tests {
    use cosmic_golden::golden_test;

    use super::*;

    // Only fonts bundled by cosmic-golden: entries render in their own face,
    // and host-installed fonts would make the goldens machine-dependent.
    const TEST_FONTS: &[&str] = &["Noto Sans", "Noto Serif", "Noto Sans Mono"];

    #[golden_test(400, 80)]
    fn font_picker_closed() -> cosmic::Element<'static, String> {
        FontPicker::new(TEST_FONTS, "Choose font…", "", |s| s)
            .selected("Noto Serif")
            .view()
    }

    #[golden_test(400, 500)]
    fn font_picker_open() -> cosmic::Element<'static, String> {
        FontPicker::new(TEST_FONTS, "Choose font…", "", |s| s)
            .selected("Noto Serif")
            .focused(true)
            .view()
    }

    #[golden_test(400, 500, dark)]
    fn font_picker_open_dark() -> cosmic::Element<'static, String> {
        FontPicker::new(TEST_FONTS, "Choose font…", "", |s| s)
            .selected("Noto Serif")
            .focused(true)
            .view()
    }

    #[golden_test(400, 500)]
    fn font_picker_filtered() -> cosmic::Element<'static, String> {
        FontPicker::new(TEST_FONTS, "Choose font…", "Ser", |s| s)
            .selected("Noto Serif")
            .focused(true)
            .view()
    }
}
