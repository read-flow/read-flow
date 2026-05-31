use iced::Element;
use iced::widget::column;
use iced::widget::rule;
use iced::widget::text;
use iced::widget::toggler;
use read_flow_core::settings::UiSettings;

use crate::app::Message;
use crate::widgets::tag_chips::tag_chips;

pub fn view_ui<'a>(ui: &'a UiSettings, private_tag_input: &'a str) -> Element<'a, Message> {
    let private_mode_row = toggler(ui.private_mode())
        .label("Private mode (hide private-tagged documents)")
        .on_toggle(Message::TogglePrivateMode);

    let tags_section = column![
        text("Private tags:").size(14),
        text("Documents with these tags are hidden when private mode is off.").size(12),
        tag_chips(
            ui.private_tags(),
            private_tag_input,
            Message::PrivateTagInput,
            Message::AddPrivateTag,
            Message::RemovePrivateTag,
        ),
    ]
    .spacing(6);

    column![
        text("UI").size(20),
        text("User interface preferences.").size(13),
        rule::horizontal(1),
        private_mode_row,
        tags_section,
    ]
    .spacing(12)
    .padding(20)
    .into()
}
