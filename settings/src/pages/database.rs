use iced::Element;
use iced::widget::button;
use iced::widget::column;
use iced::widget::row;
use iced::widget::rule;
use iced::widget::text;
use read_flow_core::db::DbSettings;

use crate::app::Message;

pub fn view_database(db: &DbSettings) -> Element<'_, Message> {
    column![
        text("Database").size(20),
        text("Location of the SQLite database file.").size(13),
        rule::horizontal(1),
        row![
            text("Database file:").width(130),
            text(db.url().to_string()).width(iced::Fill),
            button(text("Browse\u{2026}"))
                .style(button::secondary)
                .on_press(Message::PickDatabaseFile),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(12)
    .padding(20)
    .into()
}
