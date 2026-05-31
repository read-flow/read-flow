use iced::Element;
use iced::widget::button;
use iced::widget::column;
use iced::widget::row;
use iced::widget::rule;
use iced::widget::text;
use read_flow_core::settings::ServerSettings;

use crate::app::Message;
use crate::widgets::user_editor::UserForm;
use crate::widgets::user_editor::view_user_form;

pub fn view_server<'a>(
    server: &'a ServerSettings,
    user_form: Option<&'a UserForm>,
) -> Element<'a, Message> {
    let folder_row = row![
        text("Download folder:").width(140),
        text(server.download_folder.to_string()).width(iced::Fill),
        button(text("Browse\u{2026}"))
            .style(button::secondary)
            .on_press(Message::PickServerFolder),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    let users_section = view_users_section(server, user_form);

    column![
        text("Server").size(20),
        text("Settings for the read-flow server component.").size(13),
        rule::horizontal(1),
        folder_row,
        rule::horizontal(1),
        users_section,
    ]
    .spacing(12)
    .padding(20)
    .into()
}

fn view_users_section<'a>(
    server: &'a ServerSettings,
    user_form: Option<&'a UserForm>,
) -> Element<'a, Message> {
    let adding = user_form.map(|f| f.original_id.is_none()).unwrap_or(false);

    let mut user_rows: Vec<Element<'a, Message>> = server
        .authorized_users
        .iter()
        .flat_map(|(user_id, entry)| {
            let id_clone = user_id.clone();
            let id_clone2 = user_id.clone();
            let is_editing = user_form
                .and_then(|f| f.original_id.as_ref())
                .map(|id| id == user_id)
                .unwrap_or(false);

            let owner_badge: Element<'a, Message> = if entry.has_role("owner") {
                text("[owner]").size(12).into()
            } else {
                iced::widget::Space::new().into()
            };

            let header: Element<'a, Message> = row![
                text(user_id.clone()).width(iced::Fill),
                text("\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}"),
                owner_badge,
                button(text("Edit"))
                    .style(button::secondary)
                    .on_press(Message::UserEditStart(id_clone)),
                button(text("Delete"))
                    .style(button::danger)
                    .on_press(Message::UserDelete(id_clone2)),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center)
            .into();

            if is_editing {
                vec![
                    header,
                    view_user_form(
                        user_form.unwrap(),
                        Message::UserForm,
                        Message::UserSave,
                        Message::UserCancel,
                    ),
                ]
            } else {
                vec![header]
            }
        })
        .collect();

    if adding {
        user_rows.push(view_user_form(
            user_form.unwrap(),
            Message::UserForm,
            Message::UserSave,
            Message::UserCancel,
        ));
    }

    user_rows.push(
        button(text("+ Add User"))
            .style(button::secondary)
            .on_press(Message::UserAddStart)
            .into(),
    );

    column![
        text("Authorized users:").size(14),
        column(user_rows).spacing(6),
    ]
    .spacing(6)
    .into()
}
