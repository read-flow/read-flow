// SPDX-License-Identifier: AGPL-3.0-or-later

use cosmic::Apply;
use cosmic::Element;
use cosmic::Renderer;
use cosmic::Theme;
use cosmic::iced::Background;
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
use cosmic::theme;
use cosmic::widget;
use cosmic::widget::button;

/// A combo-box that combines a free-form text input with a floating overlay
/// list of predefined options.  The overlay is only shown while the input has
/// focus (controlled externally via `.focused()`).  The dropdown is drawn on
/// top of surrounding content and is constrained to the exact width of the
/// text input.
///
/// Focus lifecycle messages (`on_open` / `on_close`) let the caller track
/// focus state.  `on_select` fires when a listed option is clicked; it
/// defaults to the `on_change` callback when not set.
pub struct ComboBox<'a, Message> {
    options: &'a [String],
    placeholder: String,
    value: &'a str,
    on_change: Box<dyn Fn(String) -> Message + 'a>,
    on_select: Option<Box<dyn Fn(String) -> Message + 'a>>,
    on_open: Option<Message>,
    on_close: Option<Message>,
    on_clear: Option<Message>,
    focused: bool,
    width: Length,
}

impl<'a, Message: Clone + 'static> ComboBox<'a, Message> {
    pub fn new(
        options: &'a [String],
        placeholder: impl Into<String>,
        value: &'a str,
        on_change: impl Fn(String) -> Message + 'a,
    ) -> Self {
        Self {
            options,
            placeholder: placeholder.into(),
            value,
            on_change: Box::new(on_change),
            on_select: None,
            on_open: None,
            on_close: None,
            on_clear: None,
            focused: false,
            width: Length::Fill,
        }
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

    /// Message emitted when the clear icon button is pressed.  The button is
    /// only shown when the current value is non-empty.
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
        let visible: Vec<&String> = if self.value.is_empty() {
            self.options.iter().collect()
        } else {
            self.options
                .iter()
                .filter(|o| o.to_lowercase().contains(&lower))
                .collect()
        };

        // Pre-compute option messages before moving on_change into text-input.
        let option_messages: Vec<(String, Message)> = if let Some(ref on_select) = self.on_select {
            visible
                .into_iter()
                .map(|opt| (opt.clone(), on_select(opt.clone())))
                .collect()
        } else {
            visible
                .into_iter()
                .map(|opt| (opt.clone(), (self.on_change)(opt.clone())))
                .collect()
        };

        let has_overlay = self.focused && !option_messages.is_empty();

        let on_close = self.on_close;
        let on_change = self.on_change;

        let mut input = widget::text_input(self.placeholder, self.value)
            .on_input(on_change)
            .width(Length::Fill);

        if let Some(msg) = self.on_open {
            input = input.on_focus(msg);
        }
        if let Some(msg) = on_close.clone() {
            input = input.on_unfocus(msg);
        }
        if let Some(msg) = self.on_clear.filter(|_| !self.value.is_empty()) {
            // Use zero vertical padding so the button does not increase the
            // input height compared to a combo box without a clear button.
            let clear_icon = widget::icon::from_name("edit-clear-symbolic")
                .size(16)
                .apply(button::custom)
                .class(cosmic::theme::Button::Icon)
                .on_press(msg)
                .padding([0, 4])
                .into();
            input = input.trailing_icon(clear_icon);
        }

        // Build the dropdown element — width is set to Fill here but will be
        // replaced by the actual pixel width of the text input in the overlay
        // layout pass (see DropdownOverlay::layout).
        let rows: Vec<Element<'a, Message>> = option_messages
            .into_iter()
            .map(|(label, msg)| {
                button::custom(
                    widget::text(label)
                        .apply(widget::container)
                        // .padding([space_xxs, space_xs])
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
        .max_height(200.0)
        .class(dropdown_container_style())
        .width(Length::Fill)
        .into();

        Element::new(ComboBoxWidget {
            text_input: input.into(),
            dropdown,
            has_overlay,
            width: self.width,
        })
    }
}

// ─── Custom widget ────────────────────────────────────────────────────────────

struct ComboBoxWidget<'a, Message> {
    text_input: Element<'a, Message>,
    dropdown: Element<'a, Message>,
    has_overlay: bool,
    width: Length,
}

impl<'a, Message: Clone + 'static> Widget<Message, Theme, Renderer>
    for ComboBoxWidget<'a, Message>
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

// ─── Overlay ──────────────────────────────────────────────────────────────────

pub(crate) struct DropdownOverlay<'a, 'b, Message> {
    pub(crate) content: &'b mut Element<'a, Message>,
    pub(crate) tree: &'b mut Tree,
    pub(crate) position: Point,
    pub(crate) width: f32,
}

impl<'a, 'b, Message: Clone + 'static> overlay::Overlay<Message, Theme, Renderer>
    for DropdownOverlay<'a, 'b, Message>
{
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        // Constrain popup to exactly the text input's pixel width.
        let limits =
            layout::Limits::new(Size::ZERO, Size::new(self.width, bounds.height)).width(self.width);
        let node = self
            .content
            .as_widget_mut()
            .layout(self.tree, renderer, &limits);
        node.move_to(self.position)
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.content.as_widget().draw(
            self.tree,
            renderer,
            theme,
            style,
            layout,
            cursor,
            &layout.bounds(),
        )
    }

    fn operate(
        &mut self,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(self.tree, layout, renderer, operation);
    }

    fn update(
        &mut self,
        event: &cosmic::iced::Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        self.content.as_widget_mut().update(
            self.tree,
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            &layout.bounds(),
        );
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            self.tree,
            layout,
            cursor,
            &layout.bounds(),
            _renderer,
        )
    }
}

// ─── Styling ──────────────────────────────────────────────────────────────────

pub(crate) fn dropdown_container_style() -> cosmic::theme::Container<'static> {
    cosmic::theme::Container::custom(|theme| {
        let cosmic = theme.cosmic();
        cosmic::iced::widget::container::Style {
            background: Some(cosmic::iced::Background::Color(
                cosmic.background(false).component.base.into(),
            )),
            border: cosmic::iced::Border {
                color: cosmic.primary(false).divider.into(),
                width: 1.0,
                radius: cosmic.corner_radii.radius_s.into(),
            },
            ..Default::default()
        }
    })
}

pub(crate) fn option_button_style() -> cosmic::theme::Button {
    cosmic::theme::Button::Custom {
        active: Box::new(|_focused, theme| option_style(false, theme)),
        hovered: Box::new(|_focused, theme| option_style(true, theme)),
        pressed: Box::new(|_focused, theme| option_style(true, theme)),
        disabled: Box::new(|theme| option_style(false, theme)),
    }
}

pub(crate) fn option_style(hovered: bool, theme: &Theme) -> cosmic::widget::button::Style {
    let cosmic = theme.cosmic();
    let component = &theme.current_container().component;
    let mut style = cosmic::widget::button::Style::new();
    style.border_radius = cosmic.corner_radii.radius_s.into();
    if hovered {
        style.background = Some(Background::Color(component.hover.into()));
    }
    style.text_color = Some(component.on.into());
    style
}

#[cfg(test)]
mod tests {
    use cosmic_golden::golden_test;

    use super::*;

    fn options() -> Vec<String> {
        vec![
            "Pride and Prejudice".into(),
            "Persuasion".into(),
            "Emma".into(),
            "Sense and Sensibility".into(),
            "Northanger Abbey".into(),
        ]
    }

    #[golden_test(400, 250)]
    fn combo_box_empty_shows_all() -> Element<'_, String> {
        let opts = options();
        ComboBox::new(&opts, "Choose or type…", "", |s| s)
            .focused(true)
            .view()
    }

    #[golden_test(400, 250, dark)]
    fn combo_box_dark() -> Element<'_, String> {
        let opts = options();
        ComboBox::new(&opts, "Choose or type…", "", |s| s)
            .focused(true)
            .view()
    }

    #[golden_test(400, 250)]
    fn combo_box_filtered() -> Element<'_, String> {
        let opts = options();
        ComboBox::new(&opts, "Choose or type…", "per", |s| s)
            .focused(true)
            .view()
    }

    #[golden_test(400, 100)]
    fn combo_box_with_clear_button() -> Element<'_, String> {
        let opts = options();
        ComboBox::new(&opts, "Choose or type…", "per", |s| s)
            .on_clear(String::new())
            .view()
    }

    #[golden_test(400, 100)]
    fn combo_box_no_match_hides_list() -> Element<'_, String> {
        let opts = options();
        ComboBox::new(&opts, "Choose or type…", "xyz", |s| s)
            .focused(true)
            .view()
    }
}
