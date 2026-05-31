use iced::Element;
use iced::widget::button;
use iced::widget::column;
use iced::widget::row;
use iced::widget::rule;
use iced::widget::text;
use read_flow_core::settings::ClientSettings;

use crate::app::Message;

pub fn view_client(client: &ClientSettings) -> Element<'_, Message> {
    column![
        text("Client").size(20),
        text("Where downloaded files are saved on this machine.").size(13),
        rule::horizontal(1),
        row![
            text("Download folder:").width(140),
            text(client.download_folder.to_string()).width(iced::Fill),
            button(text("Browse\u{2026}"))
                .style(button::secondary)
                .on_press(Message::PickClientFolder),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    ]
    .spacing(12)
    .padding(20)
    .into()
}
