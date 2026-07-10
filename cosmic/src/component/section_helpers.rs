// SPDX-License-Identifier: AGPL-3.0-or-later

use cosmic::Apply;
use cosmic::Element;
use cosmic::iced::Length;
use cosmic::iced::alignment::Horizontal;
use cosmic::widget;
use cosmic::widget::icon;

use crate::ICON_SIZE;

/// A right-aligned suggested "add" icon button row for use as a settings section footer.
///
/// `on_press = None` renders a visually disabled button with no interaction.
pub fn section_add_button<'a, Msg: Clone + 'static>(
    tooltip: impl Into<String>,
    on_press: Option<Msg>,
) -> Element<'a, Msg> {
    let tooltip: String = tooltip.into();
    let btn = widget::button::icon(icon::from_name("list-add-symbolic").size(ICON_SIZE))
        .class(widget::button::ButtonClass::Suggested)
        .tooltip(tooltip);
    let btn = match on_press {
        Some(msg) => btn.on_press(msg),
        None => btn,
    };
    widget::settings::item_row(vec![
        widget::space::horizontal()
            .width(Length::FillPortion(5))
            .into(),
        btn.apply(widget::container)
            .width(Length::FillPortion(1))
            .align_x(Horizontal::Right)
            .into(),
    ])
    .into()
}
