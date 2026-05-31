use iced::Element;
use iced::widget::button;
use iced::widget::column;
use iced::widget::row;
use iced::widget::text;
use read_flow_core::db::DbSettings;

use crate::app::Message;
use crate::widgets::settings_section::settings_section;

pub fn view_database(db: &DbSettings) -> Element<'_, Message> {
    column![
        text("Database").size(20),
        text("Location of the SQLite database file.").size(13),
        settings_section(
            None,
            vec![
                row![
                    text("Database file:").width(140),
                    text(db.url().to_string()).width(iced::Fill),
                    button(text("Browse\u{2026}"))
                        .style(button::secondary)
                        .on_press(Message::PickDatabaseFile),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center)
                .into()
            ],
        ),
    ]
    .spacing(12)
    .padding(20)
    .into()
}
