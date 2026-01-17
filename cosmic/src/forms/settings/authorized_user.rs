// SPDX-License-Identifier: GPL-3.0-or-later

use archive_organizer::Builder;
use cosmic::Action;
use cosmic::Element;
use cosmic::Task;
use cosmic::iced::Length;
use cosmic::task;
use cosmic::widget;

use crate::ICON_SIZE;
use crate::fl;

pub struct AuthorizedUserForm {
    pub(crate) original_user_id: Option<String>,
    editing_user_id: String,
    editing_passphrase: String,
    show_passphrase: bool,
}

#[derive(Debug, Clone)]
pub enum AuthorizedUserFormMessage {
    EditUserId(String),
    EditPassphrase(String),
    ToggleShowPassphrase,
    Out(AuthorizedUserFormOutput),
}

#[derive(Debug, Clone)]
pub enum AuthorizedUserFormOutput {
    Submit(Option<String>, String, String), // original_user_id, editing_user_id, editing_passphrase
    Cancel,
}

impl From<AuthorizedUserFormOutput> for AuthorizedUserFormMessage {
    fn from(value: AuthorizedUserFormOutput) -> Self {
        Self::Out(value)
    }
}

impl AuthorizedUserForm {
    pub fn new(user_id: Option<String>) -> (Self, Task<Action<AuthorizedUserFormMessage>>) {
        (
            Self {
                original_user_id: user_id.clone(),
                editing_user_id: user_id.unwrap_or_default(),
                editing_passphrase: String::new(),
                show_passphrase: false,
            },
            task::none(),
        )
    }

    fn password_meets_requirements(password: &str) -> bool {
        // TODO: combination of alphanumeric and special characters
        password.len() >= 8
    }

    fn is_submittable(&self) -> bool {
        !self.editing_user_id.is_empty()
            && Self::password_meets_requirements(&self.editing_passphrase)
    }

    pub fn view(&self) -> Element<'_, AuthorizedUserFormMessage> {
        widget::settings::section()
            .title(fl!("settings-server-edit-authorized-user"))
            .add(
                widget::settings::item::builder(fl!("settings-server-user-id"))
                    .icon(widget::icon::from_name("avatar-default-symbolic").size(ICON_SIZE))
                    .control(
                        widget::text_input(
                            fl!("settings-server-user-id-placeholder"),
                            &self.editing_user_id,
                        )
                        .leading_icon(
                            widget::icon::from_name("user-info-symbolic")
                                .size(ICON_SIZE)
                                .into(),
                        )
                        .on_input(AuthorizedUserFormMessage::EditUserId)
                        .width(Length::FillPortion(1)),
                    ),
            )
            .add(
                widget::settings::item::builder(fl!("settings-server-passphrase"))
                    .icon(widget::icon::from_name("dialog-password-symbolic").size(ICON_SIZE))
                    .control(
                        widget::secure_input(
                            fl!("settings-server-passphrase-placeholder"),
                            &self.editing_passphrase,
                            Some(AuthorizedUserFormMessage::ToggleShowPassphrase),
                            !self.show_passphrase,
                        )
                        .leading_icon(
                            widget::icon::from_name("dialog-password-symbolic")
                                .size(ICON_SIZE)
                                .into(),
                        )
                        .on_input(AuthorizedUserFormMessage::EditPassphrase)
                        .width(Length::FillPortion(1)),
                    ),
            )
            .add(widget::settings::item_row(vec![
                widget::horizontal_space().width(Length::Fill).into(),
                // Cancel button
                widget::button::icon(
                    widget::icon::from_name("edit-clear-all-symbolic").size(ICON_SIZE),
                )
                .on_press(AuthorizedUserFormOutput::Cancel.into())
                .into(),
                // Submit button
                widget::button::icon(
                    widget::icon::from_name(if self.original_user_id.is_none() {
                        "list-add-symbolic"
                    } else {
                        "edit-symbolic"
                    })
                    .size(ICON_SIZE),
                )
                .class(widget::button::ButtonClass::Suggested)
                .apply_if(self.is_submittable(), |button| {
                    button.on_press(
                        AuthorizedUserFormOutput::Submit(
                            self.original_user_id.clone(),
                            self.editing_user_id.clone(),
                            self.editing_passphrase.clone(),
                        )
                        .into(),
                    )
                })
                .into(),
            ]))
            .into()
    }

    pub fn update(
        &mut self,
        message: AuthorizedUserFormMessage,
    ) -> Task<Action<AuthorizedUserFormMessage>> {
        tracing::debug!("received: {message:?}");
        match message {
            AuthorizedUserFormMessage::EditUserId(user_id) => {
                self.editing_user_id = user_id;
                task::none()
            }
            AuthorizedUserFormMessage::EditPassphrase(passphrase) => {
                self.editing_passphrase = passphrase;
                task::none()
            }
            AuthorizedUserFormMessage::ToggleShowPassphrase => {
                self.show_passphrase = !self.show_passphrase;
                task::none()
            }
            AuthorizedUserFormMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent component")
            }
        }
    }
}
