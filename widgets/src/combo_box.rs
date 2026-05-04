// SPDX-License-Identifier: GPL-3.0-or-later

use cosmic::Apply;
use cosmic::Element;
use cosmic::iced::Background;
use cosmic::iced::Length;
use cosmic::theme;
use cosmic::widget;
use cosmic::widget::button;

/// A combo-box that combines a free-form text input with a scrollable list of
/// predefined options.  The caller receives the current string value through a
/// single `on_change` callback regardless of whether the user typed a custom
/// value or clicked one of the listed options.
///
/// When the input is empty every option is shown.  When the input contains
/// text, only options whose text contains the input value (case-insensitive)
/// are shown.  If no options match the list is hidden entirely.
pub struct ComboBox<'a, Message> {
    options: &'a [String],
    placeholder: &'a str,
    value: &'a str,
    on_change: Box<dyn Fn(String) -> Message + 'a>,
    width: Length,
}

impl<'a, Message: Clone + 'static> ComboBox<'a, Message> {
    pub fn new(
        options: &'a [String],
        placeholder: &'a str,
        value: &'a str,
        on_change: impl Fn(String) -> Message + 'a,
    ) -> Self {
        Self {
            options,
            placeholder,
            value,
            on_change: Box::new(on_change),
            width: Length::Fill,
        }
    }

    pub fn width(mut self, width: Length) -> Self {
        self.width = width;
        self
    }

    pub fn view(self) -> Element<'a, Message> {
        let cosmic::cosmic_theme::Spacing {
            space_xxs,
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

        // Pre-compute one Message per visible option before moving on_change
        // into the text-input closure.
        let option_messages: Vec<(String, Message)> = visible
            .into_iter()
            .map(|opt| (opt.clone(), (self.on_change)(opt.clone())))
            .collect();

        let on_change = self.on_change;
        let input = widget::text_input(self.placeholder, self.value)
            .on_input(on_change)
            .width(Length::Fill);

        let width = self.width;

        if option_messages.is_empty() {
            return widget::container(input).width(width).into();
        }

        let rows: Vec<Element<'a, Message>> = option_messages
            .into_iter()
            .map(|(label, msg)| {
                button::custom(
                    widget::text(label)
                        .apply(widget::container)
                        .padding([space_xxs, space_xs])
                        .width(Length::Fill),
                )
                .class(option_button_style())
                .width(Length::Fill)
                .on_press(msg)
                .into()
            })
            .collect();

        let list = widget::container(widget::scrollable::vertical(widget::column::with_children(
            rows,
        )))
        .max_height(200.0)
        .class(cosmic::theme::Container::Card)
        .width(Length::Fill);

        widget::container(
            widget::column::with_children(vec![input.into(), list.into()]).spacing(space_xxs),
        )
        .width(width)
        .into()
    }
}

fn option_button_style() -> cosmic::theme::Button {
    cosmic::theme::Button::Custom {
        active: Box::new(|_focused, theme| option_style(false, theme)),
        hovered: Box::new(|_focused, theme| option_style(true, theme)),
        pressed: Box::new(|_focused, theme| option_style(true, theme)),
        disabled: Box::new(|theme| option_style(false, theme)),
    }
}

fn option_style(hovered: bool, theme: &cosmic::Theme) -> cosmic::widget::button::Style {
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
        // value="" → all options shown
        ComboBox::new(&opts, "Choose or type…", "", |s| s).view()
    }

    #[golden_test(400, 250, dark)]
    fn combo_box_dark() -> Element<'_, String> {
        let opts = options();
        ComboBox::new(&opts, "Choose or type…", "", |s| s).view()
    }

    #[golden_test(400, 250)]
    fn combo_box_filtered() -> Element<'_, String> {
        let opts = options();
        // value="per" → matches "Persuasion" only
        ComboBox::new(&opts, "Choose or type…", "per", |s| s).view()
    }

    #[golden_test(400, 100)]
    fn combo_box_no_match_hides_list() -> Element<'_, String> {
        let opts = options();
        // value="xyz" → no matches → list absent
        ComboBox::new(&opts, "Choose or type…", "xyz", |s| s).view()
    }
}
