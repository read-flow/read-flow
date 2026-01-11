use std::sync::Arc;

use archive_organizer::Builder;
use archive_organizer::api::FileDataSource;
use archive_organizer::api::Status;
use archive_organizer::client::FilesClient;
use archive_organizer::db::dao::RemoteDao;
use archive_organizer::db::models::NewRemote;
use archive_organizer::db::models::Remote;
use cosmic::Action;
use cosmic::Apply;
use cosmic::Element;
use cosmic::Task;
use cosmic::cosmic_theme;
use cosmic::iced::Length;
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::alignment::Vertical;
use cosmic::task;
use cosmic::theme;
use cosmic::widget;
use cosmic::widget::container;
use cosmic::widget::icon;
use cosmic::widget::row;
use cosmic::widget::settings;
use url::Url;

use crate::ApplicationModule;
use crate::ICON_SIZE;
use crate::app::ContextView;
use crate::fl;
use crate::iter::find_with_next;
use crate::iter::find_with_previous;
use crate::layout::layout;
use crate::state::LoadedState;

pub type RemotesState = LoadedState<Vec<Remote>>;
pub type UrlVerificationState = LoadedState<Status>;

pub struct SourcesPage {
    application_module: Arc<ApplicationModule>,
    remotes_state: RemotesState,
    entered_url: String,
    entered_url_id: widget::Id, // Unique ID for focus management
    entered_token: String,
    entered_token_id: widget::Id, // Unique ID for focus management
    url_verification_state: UrlVerificationState,
    operation_error: Option<String>,
    pending_deletion: Option<Remote>,
}

#[derive(Debug, Clone)]
pub enum SourcesOutput {
    AddedSource(Url, String),
    DeletedSource(Url),
}

#[derive(Debug, Clone)]
pub enum SourcesMessage {
    LoadRemotes,
    SetRemotesStateFailed(String),
    SetRemotesStateLoaded(Vec<Remote>),

    UpdateEnteredUrl(String),
    UpdateEnteredToken(String),
    VerifyEnteredUrl {
        url: Url,
        token: String,
        do_submit: bool,
    },
    SetUrlVerificationStateFailed(String),
    SetUrlVerificationStateLoaded(Status),
    ClearUrlEntries,

    AddSource(String),
    SubmitSource(Url, String),
    SubmittedSource(Url, String),
    RequestDeleteSource(Remote),
    ConfirmDeleteSource,
    CancelDeleteSource,
    DeleteSource(i32),
    DeletedSource(i32),

    SetOperationError(String),
    ClearOperationError,

    MoveSourceUp(Remote),
    MoveSourceDown(Remote),
    SwapOrderOfRemotes(Remote, Remote),

    Out(SourcesOutput),
}

impl SourcesPage {
    pub fn new(application_module: Arc<ApplicationModule>) -> (Self, Task<Action<SourcesMessage>>) {
        (
            Self {
                application_module,
                remotes_state: Default::default(),
                entered_url: Default::default(),
                entered_url_id: widget::Id::unique(),
                entered_token: Default::default(),
                entered_token_id: widget::Id::unique(),
                url_verification_state: Default::default(),
                operation_error: None,
                pending_deletion: None,
            },
            task::message(SourcesMessage::LoadRemotes),
        )
    }

    pub fn view(&self) -> Element<'_, SourcesMessage> {
        let cosmic_theme::Spacing {
            space_s, space_xs, ..
        } = theme::active().cosmic().spacing;

        let mut content = Vec::new();

        // Show confirmation dialog if there's a pending deletion
        if let Some(remote) = &self.pending_deletion {
            let dialog = widget::dialog()
                .title(fl!("sources-delete-confirm-title"))
                .body(fl!("sources-delete-confirm-body"))
                .icon(icon::from_name("dialog-warning-symbolic").size(64))
                .control(
                    widget::text::monotext(&remote.base_url)
                        .apply(widget::container)
                        .class(cosmic::theme::Container::Card)
                        .padding(space_s)
                        .width(Length::Fill),
                )
                .primary_action(
                    widget::button::destructive(fl!("sources-delete-confirm-delete"))
                        .on_press(SourcesMessage::ConfirmDeleteSource),
                )
                .secondary_action(
                    widget::button::standard(fl!("sources-delete-confirm-cancel"))
                        .on_press(SourcesMessage::CancelDeleteSource),
                );

            content.push(
                row()
                    .push(widget::horizontal_space())
                    .push(dialog.width(Length::FillPortion(10)))
                    .push(widget::horizontal_space())
                    .into(),
            );
        }

        // Show error dialog if there's an operation error
        if let Some(error) = &self.operation_error {
            let dialog = widget::dialog()
                .title(fl!("sources-error-title"))
                .control(
                    widget::text::monotext(error)
                        .apply(widget::container)
                        .class(cosmic::theme::Container::Card)
                        .padding(space_s)
                        .width(Length::Fill),
                )
                .icon(icon::from_name("dialog-error-symbolic").size(64))
                .primary_action(
                    widget::button::suggested(fl!("sources-error-close"))
                        .on_press(SourcesMessage::ClearOperationError),
                );

            content.push(
                row()
                    .push(widget::horizontal_space())
                    .push(dialog.width(Length::FillPortion(10)))
                    .push(widget::horizontal_space())
                    .into(),
            );
        }

        // Sources list section
        let sources_section = match &self.remotes_state {
            LoadedState::New => settings::section()
                .title(fl!("sources-section-title"))
                .add(widget::text(fl!("sources-loading-state-new")))
                .into(),
            LoadedState::Loading => settings::section()
                .title(fl!("sources-section-title"))
                .add(widget::text(fl!("sources-loading-state-loading")))
                .into(),
            LoadedState::Failed(error) => settings::section()
                .title(fl!("sources-section-title"))
                .add(widget::text(fl!("generic-error", error = error)))
                .into(),
            LoadedState::Loaded(sources) => {
                if sources.is_empty() {
                    settings::section()
                        .title(fl!("sources-section-title"))
                        .add(widget::text(fl!("sources-empty-state")))
                        .into()
                } else {
                    sources
                        .iter()
                        .enumerate()
                        .fold(
                            settings::section().title(fl!("sources-section-title")),
                            |section, (index, source)| {
                                section.add(self.view_source(
                                    source,
                                    index == 0,
                                    index == sources.len() - 1,
                                ))
                            },
                        )
                        .into()
                }
            }
        };

        content.push(sources_section);

        // Add source input section
        let add_section = settings::section()
            .title(fl!("sources-add-section-title"))
            .add(widget::settings::item(
                fl!("sources-url"),
                row()
                    .push(
                        widget::text_input(fl!("sources-url-placeholder"), &self.entered_url)
                            .id(self.entered_url_id.clone())
                            .on_input(SourcesMessage::UpdateEnteredUrl)
                            .width(Length::Fill),
                    )
                    .push(
                        match self.url_verification_state {
                            LoadedState::New => icon::from_name("dialog-information-symbolic"),
                            LoadedState::Loading => icon::from_name("dialog-question-symbolic"),
                            LoadedState::Failed(_) => icon::from_name("dialog-error-symbolic"),
                            LoadedState::Loaded(_) => icon::from_name("emblem-ok-symbolic"),
                        }
                        .size(ICON_SIZE),
                    )
                    .spacing(space_xs)
                    .align_y(Vertical::Center)
                    .width(Length::Fixed(600.0)),
            ))
            .add(widget::settings::item(
                fl!("sources-authorization-token"),
                widget::text_input(
                    fl!("sources-authorization-token-placeholder"),
                    &self.entered_token,
                )
                .id(self.entered_token_id.clone())
                .on_input(SourcesMessage::UpdateEnteredToken)
                .password()
                .width(Length::Fixed(600.0)),
            ))
            .add_maybe(
                matches!(self.url_verification_state, LoadedState::Failed(_)).then(|| {
                    let LoadedState::Failed(ref error) = self.url_verification_state else {
                        unreachable!()
                    };
                    widget::text::caption(error)
                }),
            )
            .add(
                row()
                    .push(widget::horizontal_space().width(Length::Fill))
                    .push(
                        widget::button::suggested(fl!("sources-add-button")).apply_if(
                            !(self.entered_url.is_empty()
                                || self.entered_token.is_empty()
                                || !matches!(self.url_verification_state, LoadedState::Loaded(_))),
                            |button| {
                                button.on_press(SourcesMessage::AddSource(self.entered_url.clone()))
                            },
                        ),
                    ),
            );

        content.push(add_section.into());

        layout(settings::view_column(content))
            .apply(widget::scrollable::vertical)
            .apply(widget::container)
            .height(Length::Fill)
            .align_x(Horizontal::Center)
            .align_y(Vertical::Top)
            .into()
    }

    pub fn view_context(&self) -> ContextView<'_, SourcesMessage> {
        ContextView {
            title: "Sources".to_string(),
            content: widget::text("TODO").into(),
        }
    }

    pub fn update(&mut self, message: SourcesMessage) -> Task<Action<SourcesMessage>> {
        tracing::debug!("received: {message:?}");
        match message {
            SourcesMessage::LoadRemotes => {
                self.remotes_state = RemotesState::Loading;
                let connection_pool = self.application_module.connection_pool();
                task::future(async move {
                    match connection_pool.select_all_remotes() {
                        Ok(remotes) => SourcesMessage::SetRemotesStateLoaded(remotes),
                        Err(error) => SourcesMessage::SetRemotesStateFailed(format!("{error}")),
                    }
                })
            }
            SourcesMessage::SetRemotesStateLoaded(remotes) => {
                self.remotes_state = RemotesState::Loaded(remotes);
                task::none()
            }
            SourcesMessage::SetRemotesStateFailed(error) => {
                self.remotes_state = RemotesState::Failed(error);
                task::none()
            }
            SourcesMessage::UpdateEnteredUrl(url) => {
                self.entered_url = url;
                self.url_verification_state = UrlVerificationState::New;
                if self.entered_url.is_empty() {
                    widget::text_input::focus(self.entered_url_id.clone())
                } else {
                    match self.entered_url.parse::<Url>() {
                        Ok(url) => task::message(SourcesMessage::VerifyEnteredUrl {
                            url,
                            token: self.entered_token.clone(),
                            do_submit: false,
                        }),
                        Err(_) => task::message(SourcesMessage::SetUrlVerificationStateFailed(
                            fl!("sources-invalid-url"),
                        )),
                    }
                }
            }
            SourcesMessage::UpdateEnteredToken(token) => {
                self.entered_token = token;
                task::none()
            }
            SourcesMessage::SetUrlVerificationStateFailed(error) => {
                self.url_verification_state = UrlVerificationState::Failed(error);
                widget::text_input::focus(self.entered_url_id.clone())
            }
            SourcesMessage::VerifyEnteredUrl {
                url,
                token,
                do_submit,
            } => {
                self.url_verification_state = UrlVerificationState::Loading;
                let client = FilesClient::new(url.clone(), token.clone()).expect("valid url");
                Task::batch(vec![
                    widget::text_input::focus(self.entered_url_id.clone()),
                    task::future(async move {
                        match client.status().await {
                            Ok(_status) if do_submit => SourcesMessage::SubmitSource(url, token),
                            Ok(status) => SourcesMessage::SetUrlVerificationStateLoaded(status),
                            Err(error) => {
                                SourcesMessage::SetUrlVerificationStateFailed(format!("{error}"))
                            }
                        }
                    }),
                ])
            }
            SourcesMessage::SetUrlVerificationStateLoaded(status) => {
                self.url_verification_state = UrlVerificationState::Loaded(status);
                widget::text_input::focus(self.entered_url_id.clone())
            }
            SourcesMessage::AddSource(url) => {
                self.entered_url = url;
                match self.entered_url.parse::<Url>() {
                    Ok(url) => task::message(SourcesMessage::VerifyEnteredUrl {
                        url,
                        token: self.entered_token.clone(),
                        do_submit: true,
                    }),
                    Err(_) => task::message(SourcesMessage::SetUrlVerificationStateFailed(
                        String::from("invalid-url"),
                    )),
                }
            }
            SourcesMessage::SubmitSource(url, token) => {
                let connection_pool = self.application_module.connection_pool();
                let order = self.remotes_state.unwrap().len() + 1;
                task::future(async move {
                    match connection_pool.insert_remote(NewRemote {
                        base_url: url.to_string(),
                        order: order as i32,
                        authorization_token: token.clone(),
                    }) {
                        Ok(_) => SourcesMessage::SubmittedSource(url, token),
                        Err(error) => SourcesMessage::SetOperationError(format!("{error}")),
                    }
                })
            }
            SourcesMessage::SubmittedSource(url, token) => {
                task::message(SourcesMessage::Out(SourcesOutput::AddedSource(url, token)))
                    .chain(task::message(SourcesMessage::ClearUrlEntries))
            }
            SourcesMessage::ClearUrlEntries => {
                self.entered_url.clear();
                self.entered_token.clear();
                self.url_verification_state = Default::default();
                task::message(SourcesMessage::LoadRemotes)
            }
            SourcesMessage::RequestDeleteSource(remote) => {
                self.pending_deletion = Some(remote);
                task::none()
            }
            SourcesMessage::ConfirmDeleteSource => {
                if let Some(remote) = self.pending_deletion.take() {
                    task::message(SourcesMessage::DeleteSource(remote.id))
                } else {
                    task::none()
                }
            }
            SourcesMessage::CancelDeleteSource => {
                self.pending_deletion = None;
                task::none()
            }
            SourcesMessage::DeleteSource(id) => {
                let connection_pool = self.application_module.connection_pool();
                task::future(async move {
                    match connection_pool.delete_remote_by_id(id) {
                        Ok(_) => SourcesMessage::DeletedSource(id),
                        Err(error) => SourcesMessage::SetOperationError(format!("{error}")),
                    }
                })
            }
            SourcesMessage::DeletedSource(id) => {
                let remote = self
                    .remotes_state
                    .unwrap() // should be safe, because otherwise `DeleteSource` message cannot be generated.
                    .iter()
                    .find(|a| a.id == id)
                    .unwrap(); // should be safe, because the source should exist.

                task::message(SourcesMessage::Out(SourcesOutput::DeletedSource(
                    remote.base_url.parse().unwrap(),
                )))
                .chain(task::message(SourcesMessage::LoadRemotes))
            }
            SourcesMessage::SetOperationError(error) => {
                self.operation_error = Some(error);
                task::none()
            }
            SourcesMessage::ClearOperationError => {
                self.operation_error = None;
                task::none()
            }
            SourcesMessage::MoveSourceUp(remote) => {
                find_with_previous(self.remotes_state.unwrap().iter(), |current| {
                    current.id == remote.id
                })
                .map(|(prev, current)| {
                    task::message(SourcesMessage::SwapOrderOfRemotes(
                        prev.clone(),
                        current.clone(),
                    ))
                })
                .unwrap_or_else(task::none)
            }
            SourcesMessage::MoveSourceDown(remote) => {
                find_with_next(self.remotes_state.unwrap().iter(), |current| {
                    current.id == remote.id
                })
                .map(|(current, next)| {
                    task::message(SourcesMessage::SwapOrderOfRemotes(
                        current.clone(),
                        next.clone(),
                    ))
                })
                .unwrap_or_else(task::none)
            }
            SourcesMessage::SwapOrderOfRemotes(first, second) => {
                let connection_pool = self.application_module.connection_pool();
                task::future(async move {
                    match connection_pool.swap_order_of_remotes(&first, &second) {
                        Ok(_) => SourcesMessage::LoadRemotes,
                        Err(error) => SourcesMessage::SetOperationError(format!("{error}")),
                    }
                })
            }
            SourcesMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent component")
            }
        }
    }

    fn view_source<'a>(
        &self,
        source: &'a Remote,
        is_first: bool,
        is_last: bool,
    ) -> Element<'a, SourcesMessage> {
        let cosmic_theme::Spacing {
            space_xxs,
            space_xs,
            ..
        } = theme::active().cosmic().spacing;

        row()
            .push(
                icon::from_name("network-server-symbolic")
                    .size(ICON_SIZE)
                    .apply(container)
                    .padding([0, space_xs, 0, 0]),
            )
            .push(widget::text(&source.base_url).width(Length::Fill))
            .push(
                row()
                    .push(
                        widget::button::icon(icon::from_name("go-up-symbolic").size(ICON_SIZE))
                            .padding(space_xxs)
                            .class(theme::Button::Icon)
                            .apply_if(!is_first, |button| {
                                button.on_press(SourcesMessage::MoveSourceUp(source.clone()))
                            }),
                    )
                    .push(
                        widget::button::icon(icon::from_name("go-down-symbolic").size(ICON_SIZE))
                            .padding(space_xxs)
                            .class(theme::Button::Icon)
                            .apply_if(!is_last, |button| {
                                button.on_press(SourcesMessage::MoveSourceDown(source.clone()))
                            }),
                    )
                    .push(
                        widget::button::icon(
                            icon::from_name("list-remove-symbolic").size(ICON_SIZE),
                        )
                        .padding(space_xxs)
                        .class(theme::Button::Destructive)
                        .on_press(SourcesMessage::RequestDeleteSource(source.clone())),
                    )
                    .spacing(space_xxs),
            )
            .spacing(space_xs)
            .align_y(Vertical::Center)
            .into()
    }
}
