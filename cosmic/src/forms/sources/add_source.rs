// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::Duration;
use std::time::Instant;

use cosmic::Action;
use cosmic::Element;
use cosmic::Task;
use cosmic::iced::Length;
use cosmic::task;
use cosmic::widget;
use cosmic::widget::icon;
use cosmic::widget::settings;
use read_flow_core::Builder;
use read_flow_core::api::FileDataSource;
use read_flow_core::api::Status;
use read_flow_core::client::FilesClient;
use read_flow_core::db::models::Remote;
use url::Url;

use crate::ICON_SIZE;
use crate::fl;
use crate::state::LoadedState;

type UrlVerificationState = LoadedState<Status>;

const DEBOUNCE_TIMEOUT: Duration = Duration::from_millis(250);

pub struct AddSourceForm {
    original: Option<Remote>,
    entered_url: String,
    entered_url_id: widget::Id,
    entered_user_id: String,
    entered_user_id_id: widget::Id,
    entered_passphrase: String,
    entered_passphrase_id: widget::Id,
    show_passphrase: bool,
    url_verification_state: UrlVerificationState,
    unavailable_acknowledged: bool,
    last_input_time: Instant,
}

#[derive(Debug, Clone)]
pub enum AddSourceFormMessage {
    UpdateUrl(String),
    UpdateUserId(String),
    UpdatePassphrase(String),
    DebounceVerify(widget::Id),
    ToggleShowPassphrase,
    ToggleUnavailableAcknowledged,
    VerifyUrl {
        url: Url,
        user_id: String,
        passphrase: String,
        widget: widget::Id,
    },
    SetVerificationFailed(widget::Id, String),
    SetVerificationLoaded(widget::Id, Status),
    RequestSubmit,
    Out(AddSourceFormOutput),
}

#[derive(Debug, Clone)]
pub enum AddSourceFormOutput {
    Cancel,
    Submit(Option<Remote>, Box<Url>, String, String), // original (None=add, Some=edit), url, user_id, passphrase
}

impl From<AddSourceFormOutput> for AddSourceFormMessage {
    fn from(value: AddSourceFormOutput) -> Self {
        Self::Out(value)
    }
}

impl AddSourceForm {
    pub fn new(original: Option<&Remote>) -> (Self, Task<Action<AddSourceFormMessage>>) {
        let entered_url_id = widget::Id::unique();
        let focus_task = widget::text_input::focus(entered_url_id.clone());
        let form = Self {
            original: original.cloned(),
            entered_url: original.map(|r| r.base_url.clone()).unwrap_or_default(),
            entered_url_id,
            entered_user_id: original.map(|r| r.user_id.clone()).unwrap_or_default(),
            entered_user_id_id: widget::Id::unique(),
            entered_passphrase: original.map(|r| r.passphrase.clone()).unwrap_or_default(),
            entered_passphrase_id: widget::Id::unique(),
            show_passphrase: false,
            url_verification_state: Default::default(),
            // Allow immediate submit when editing a pre-filled form
            unavailable_acknowledged: original.is_some(),
            last_input_time: Instant::now(),
        };
        (form, focus_task)
    }

    fn is_editing(&self) -> bool {
        self.original.is_some()
    }

    fn start_debounce(&mut self, widget_id: widget::Id) -> Task<Action<AddSourceFormMessage>> {
        self.last_input_time = Instant::now();
        task::future(async move {
            tokio::time::sleep(DEBOUNCE_TIMEOUT).await;
            AddSourceFormMessage::DebounceVerify(widget_id)
        })
    }

    fn verify_url(&mut self, widget: widget::Id) -> Task<Action<AddSourceFormMessage>> {
        self.url_verification_state = UrlVerificationState::New;
        if self.entered_url.is_empty()
            || self.entered_user_id.is_empty()
            || self.entered_passphrase.is_empty()
        {
            widget::text_input::focus(widget)
        } else {
            match self.entered_url.parse::<Url>() {
                Ok(url) => task::message(AddSourceFormMessage::VerifyUrl {
                    url,
                    user_id: self.entered_user_id.clone(),
                    passphrase: self.entered_passphrase.clone(),
                    widget,
                }),
                Err(_) => task::message(AddSourceFormMessage::SetVerificationFailed(
                    widget,
                    fl!("sources-invalid-url"),
                )),
            }
        }
    }

    pub fn view(&self) -> Element<'_, AddSourceFormMessage> {
        let can_submit = !(self.entered_url.is_empty()
            || self.entered_user_id.is_empty()
            || self.entered_passphrase.is_empty()
            || self.entered_url.parse::<Url>().is_err())
            && (matches!(self.url_verification_state, LoadedState::Loaded(_))
                || self.unavailable_acknowledged);

        let fields_filled = !self.entered_url.is_empty()
            && !self.entered_user_id.is_empty()
            && !self.entered_passphrase.is_empty();
        let unverified = matches!(
            self.url_verification_state,
            LoadedState::Failed(_) | LoadedState::New | LoadedState::Loading
        );

        let section_title = if self.is_editing() {
            fl!("sources-edit-section-title")
        } else {
            fl!("sources-add-section-title")
        };

        let submit_icon = if self.is_editing() {
            "edit-symbolic"
        } else {
            "list-add-symbolic"
        };

        settings::section()
            .title(section_title)
            .add(
                widget::settings::item::builder(fl!("sources-url"))
                    .icon(icon::from_name("network-server-symbolic").size(ICON_SIZE))
                    .control(
                        widget::settings::item_row(vec![
                            widget::text_input(fl!("sources-url-placeholder"), &self.entered_url)
                                .id(self.entered_url_id.clone())
                                .on_input(AddSourceFormMessage::UpdateUrl)
                                .width(Length::Fill)
                                .into(),
                            match self.url_verification_state {
                                LoadedState::New => icon::from_name("dialog-information-symbolic"),
                                LoadedState::Loading => {
                                    icon::from_name("emblem-synchronizing-symbolic")
                                }
                                LoadedState::Failed(_) => icon::from_name("dialog-error-symbolic"),
                                LoadedState::Loaded(_) => icon::from_name("emblem-ok-symbolic"),
                            }
                            .size(ICON_SIZE)
                            .into(),
                        ])
                        .width(Length::Fill),
                    ),
            )
            .add(
                widget::settings::item::builder(fl!("sources-user-id"))
                    .icon(widget::icon::from_name("avatar-default-symbolic").size(ICON_SIZE))
                    .control(
                        widget::text_input(
                            fl!("sources-user-id-placeholder"),
                            &self.entered_user_id,
                        )
                        .id(self.entered_user_id_id.clone())
                        .on_input(AddSourceFormMessage::UpdateUserId)
                        .width(Length::Fill),
                    ),
            )
            .add(
                widget::settings::item::builder(fl!("sources-authorization-token"))
                    .icon(widget::icon::from_name("dialog-password-symbolic").size(ICON_SIZE))
                    .control(
                        widget::secure_input(
                            fl!("sources-authorization-token-placeholder"),
                            &self.entered_passphrase,
                            Some(AddSourceFormMessage::ToggleShowPassphrase),
                            !self.show_passphrase,
                        )
                        .id(self.entered_passphrase_id.clone())
                        .on_input(AddSourceFormMessage::UpdatePassphrase)
                        .width(Length::Fill),
                    ),
            )
            .add_maybe(
                matches!(self.url_verification_state, LoadedState::Failed(_)).then(|| {
                    let LoadedState::Failed(ref error) = self.url_verification_state else {
                        unreachable!()
                    };
                    widget::settings::item::builder(error.as_str())
                        .icon(icon::from_name("dialog-error-symbolic").size(ICON_SIZE))
                        .control(widget::Space::new())
                }),
            )
            .add_maybe((fields_filled && unverified).then(|| {
                widget::settings::item::builder(fl!("sources-add-unavailable-warning"))
                    .icon(icon::from_name("dialog-warning-symbolic").size(ICON_SIZE))
                    .control(
                        widget::checkbox(self.unavailable_acknowledged)
                            .on_toggle(|_| AddSourceFormMessage::ToggleUnavailableAcknowledged),
                    )
            }))
            .add(widget::settings::item_row(vec![
                widget::space::horizontal().width(Length::Fill).into(),
                widget::button::icon(icon::from_name("edit-clear-all-symbolic").size(ICON_SIZE))
                    .on_press(AddSourceFormOutput::Cancel.into())
                    .into(),
                widget::button::icon(icon::from_name(submit_icon).size(ICON_SIZE))
                    .class(widget::button::ButtonClass::Suggested)
                    .apply_if(can_submit, |b| {
                        b.on_press(AddSourceFormMessage::RequestSubmit)
                    })
                    .into(),
            ]))
            .into()
    }

    pub fn update(&mut self, message: AddSourceFormMessage) -> Task<Action<AddSourceFormMessage>> {
        tracing::debug!("received: {message:?}");
        match message {
            AddSourceFormMessage::UpdateUrl(url) => {
                self.entered_url = url;
                self.unavailable_acknowledged = false;
                self.start_debounce(self.entered_url_id.clone())
            }
            AddSourceFormMessage::UpdateUserId(user_id) => {
                self.entered_user_id = user_id;
                self.unavailable_acknowledged = false;
                self.start_debounce(self.entered_user_id_id.clone())
            }
            AddSourceFormMessage::UpdatePassphrase(passphrase) => {
                self.entered_passphrase = passphrase;
                self.unavailable_acknowledged = false;
                self.start_debounce(self.entered_passphrase_id.clone())
            }
            AddSourceFormMessage::DebounceVerify(widget_id) => {
                if self.last_input_time.elapsed() >= DEBOUNCE_TIMEOUT {
                    self.verify_url(widget_id)
                } else {
                    task::none()
                }
            }
            AddSourceFormMessage::ToggleShowPassphrase => {
                self.show_passphrase = !self.show_passphrase;
                task::none()
            }
            AddSourceFormMessage::ToggleUnavailableAcknowledged => {
                self.unavailable_acknowledged = !self.unavailable_acknowledged;
                task::none()
            }
            AddSourceFormMessage::VerifyUrl {
                url,
                user_id,
                passphrase,
                widget,
            } => {
                self.url_verification_state = UrlVerificationState::Loading;
                let client =
                    FilesClient::new(url.clone(), user_id.clone(), passphrase.clone(), false)
                        .expect("valid url");
                Task::batch(vec![
                    widget::text_input::focus(widget.clone()),
                    task::future(async move {
                        match client.status().await {
                            Ok(status) => {
                                AddSourceFormMessage::SetVerificationLoaded(widget, status)
                            }
                            Err(error) => AddSourceFormMessage::SetVerificationFailed(
                                widget,
                                format!("{error}"),
                            ),
                        }
                    }),
                ])
            }
            AddSourceFormMessage::SetVerificationFailed(widget, error) => {
                self.url_verification_state = UrlVerificationState::Failed(error);
                widget::text_input::focus(widget)
            }
            AddSourceFormMessage::SetVerificationLoaded(widget, status) => {
                self.url_verification_state = UrlVerificationState::Loaded(status);
                widget::text_input::focus(widget)
            }
            AddSourceFormMessage::RequestSubmit => match self.entered_url.parse::<Url>() {
                Ok(url) => task::message(AddSourceFormMessage::Out(AddSourceFormOutput::Submit(
                    self.original.clone(),
                    Box::new(url),
                    self.entered_user_id.clone(),
                    self.entered_passphrase.clone(),
                ))),
                Err(_) => task::message(AddSourceFormMessage::SetVerificationFailed(
                    self.entered_url_id.clone(),
                    fl!("sources-invalid-url"),
                )),
            },
            AddSourceFormMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent component")
            }
        }
    }
}
