use archive_organizer::Builder;
use archive_organizer::api::{FileDataSource, Status};
use archive_organizer::client::FilesClient;
use archive_organizer::db::ConnectionPool;
use archive_organizer::db::dao::RemoteDao;
use archive_organizer::db::models::{NewRemote, Remote};
use cosmic::iced::Length;
use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::theme::Container;
use cosmic::widget::{Column, Row, column, container, icon, row};
use cosmic::{Action, widget};
use cosmic::{Apply, Element, Task};
use cosmic::{cosmic_theme, task, theme};
use url::Url;

use crate::app::ContextView;
use crate::fl;
use crate::state::LoadedState;

pub type RemotesState = LoadedState<Vec<Remote>>;
pub type UrlVerificationState = LoadedState<Status>;

pub struct SourcesPage {
    connection_pool: ConnectionPool,
    remotes_state: RemotesState,
    entered_url: String,
    entered_url_id: widget::Id, // Unique ID for focus management
    url_verification_state: UrlVerificationState,
    operation_error: Option<String>,
    pending_deletion: Option<Remote>,
}

#[derive(Debug, Clone)]
pub enum SourcesOutput {
    AddedSource(Url),
    DeletedSource(Url),
}

#[derive(Debug, Clone)]
pub enum SourcesMessage {
    LoadRemotes,
    SetRemotesStateFailed(String),
    SetRemotesStateLoaded(Vec<Remote>),

    UpdateEnteredUrl(String),
    VerifyEnteredUrl { url: Url, do_submit: bool },
    SetUrlVerificationStateFailed(String),
    SetUrlVerificationStateLoaded(Status),
    ClearUrlEntries,

    AddSource(String),
    SubmitSource(Url),
    SubmittedSource(Url),
    RequestDeleteSource(Remote),
    ConfirmDeleteSource,
    CancelDeleteSource,
    DeleteSource(i32),
    DeletedSource(i32),

    SetOperationError(String),
    ClearOperationError,

    Out(SourcesOutput),
}

impl SourcesPage {
    pub fn new(connection_pool: ConnectionPool) -> (Self, Task<Action<SourcesMessage>>) {
        (
            Self {
                connection_pool,
                remotes_state: Default::default(),
                entered_url: Default::default(),
                entered_url_id: widget::Id::unique(),
                url_verification_state: Default::default(),
                operation_error: None,
                pending_deletion: None,
            },
            task::message(SourcesMessage::LoadRemotes),
        )
    }

    pub fn view(&self) -> Element<'_, SourcesMessage> {
        let cosmic_theme::Spacing { space_s, .. } = theme::active().cosmic().spacing;

        let mut col = column();

        // Show confirmation dialog if there's a pending deletion
        col = col.push_maybe(self.pending_deletion.as_ref().map(|remote| {
            let dialog = widget::dialog()
                .title(fl!("sources-delete-confirm-title"))
                .body(fl!("sources-delete-confirm-body"))
                .icon(icon::from_name("dialog-warning-symbolic").size(64))
                .control(
                    widget::text(&remote.base_url)
                        .font(cosmic::font::Font::MONOSPACE)
                        .apply(widget::container)
                        .class(cosmic::theme::Container::Card)
                        .padding(space_s)
                        .width(Length::Fill)
                )
                .primary_action(
                    widget::button::destructive(fl!("sources-delete-confirm-delete"))
                        .on_press(SourcesMessage::ConfirmDeleteSource),
                )
                .secondary_action(
                    widget::button::standard(fl!("sources-delete-confirm-cancel"))
                        .on_press(SourcesMessage::CancelDeleteSource),
                );

            row()
                .push(widget::text("").width(Length::FillPortion(1)))
                .push(dialog.width(Length::FillPortion(10)))
                .push(widget::text("").width(Length::FillPortion(1)))
        }));

        col = col.push_maybe(self.operation_error.as_ref().map(|error| {
            let card = column()
                .push(widget::text::heading(fl!("sources-error-card")))
                .push(widget::divider::horizontal::default())
                .push(widget::text::body(error))
                .push(widget::divider::horizontal::default())
                .push(
                    Column::new()
                        .push(
                            widget::button::text(fl!("sources-error-card-close"))
                                .on_press(SourcesMessage::ClearOperationError),
                        )
                        .width(Length::Fill)
                        .align_x(Horizontal::Right),
                )
                .padding([0, space_s])
                .spacing(space_s)
                .apply(container)
                .class(Container::Dialog)
                .padding([0, space_s]);
            row()
                .push(widget::text("").width(Length::FillPortion(1)))
                .push(card.width(Length::FillPortion(10)))
                .push(widget::text("").width(Length::FillPortion(1)))
        }));

        col = match &self.remotes_state {
            LoadedState::New => col.push(widget::text(fl!("sources-loading-state-new"))),
            LoadedState::Loading => col.push(widget::text(fl!("sources-loading-state-loading"))),
            LoadedState::Failed(error) => {
                col.push(widget::text(fl!("generic-error", error = error.as_str())))
            }
            LoadedState::Loaded(sources) => {
                let content = sources.iter().map(|source| self.view_source(source));

                col.extend(content)
            }
        };

        let input = row()
            .push(
                icon::from_name("network-server-symbolic")
                    .size(24)
                    .apply(container)
                    .align_x(Horizontal::Center)
                    .align_y(Vertical::Center)
                    .width(Length::FillPortion(1)),
            )
            .push(
                column()
                    .push(
                        widget::text_input(fl!("sources-enter-url"), &self.entered_url)
                            .id(self.entered_url_id.clone())
                            .always_active()
                            .on_input(SourcesMessage::UpdateEnteredUrl)
                            .apply_if(self.url_verification_state.is_loaded(), |input| {
                                input.on_submit(SourcesMessage::AddSource)
                            })
                            .width(Length::FillPortion(10)),
                    )
                    .apply_if(
                        matches!(self.url_verification_state, LoadedState::Failed(_)),
                        |col| {
                            let LoadedState::Failed(ref error) = self.url_verification_state else {
                                unreachable!()
                            };
                            col.push(widget::text(fl!("generic-error", error = error.as_str())))
                        },
                    ),
            )
            .push(
                match self.url_verification_state {
                    LoadedState::New => icon::from_name("dialog-information-symbolic"),
                    LoadedState::Loading => icon::from_name("dialog-question-symbolic"),
                    LoadedState::Failed(_) => icon::from_name("dialog-error-symbolic"),
                    LoadedState::Loaded(_) => icon::from_name("emblem-ok-symbolic"),
                }
                .size(24)
                .apply(container)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Center)
                .width(Length::FillPortion(1)),
            )
            .padding([0, space_s])
            .spacing(space_s)
            .align_y(Vertical::Top)
            .height(Length::Shrink);

        col.push(input).spacing(space_s).into()
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
                let connection_pool = self.connection_pool.clone();
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
                match self.entered_url.parse::<Url>() {
                    Ok(url) => task::message(SourcesMessage::VerifyEnteredUrl {
                        url,
                        do_submit: false,
                    }),
                    Err(_) => task::message(SourcesMessage::SetUrlVerificationStateFailed(fl!(
                        "sources-invalid-url"
                    ))),
                }
            }
            SourcesMessage::SetUrlVerificationStateFailed(error) => {
                self.url_verification_state = UrlVerificationState::Failed(error);
                widget::text_input::focus(self.entered_url_id.clone())
            }
            SourcesMessage::VerifyEnteredUrl { url, do_submit } => {
                self.url_verification_state = UrlVerificationState::Loading;
                let client = FilesClient::new(url.clone()).expect("valid url");
                Task::batch(vec![
                    widget::text_input::focus(self.entered_url_id.clone()),
                    task::future(async move {
                        match client.status().await {
                            Ok(_status) if do_submit => SourcesMessage::SubmitSource(url),
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
                        do_submit: true,
                    }),
                    Err(_) => task::message(SourcesMessage::SetUrlVerificationStateFailed(
                        String::from("invalid-url"),
                    )),
                }
            }
            SourcesMessage::SubmitSource(url) => {
                let connection_pool = self.connection_pool.clone();
                task::future(async move {
                    match connection_pool.insert_remote(NewRemote {
                        base_url: url.to_string(),
                    }) {
                        Ok(_) => SourcesMessage::SubmittedSource(url),
                        Err(error) => SourcesMessage::SetOperationError(format!("{error}")),
                    }
                })
            }
            SourcesMessage::SubmittedSource(url) => {
                task::message(SourcesMessage::Out(SourcesOutput::AddedSource(url)))
                    .chain(task::message(SourcesMessage::ClearUrlEntries))
            }
            SourcesMessage::ClearUrlEntries => {
                self.entered_url.clear();
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
                let connection_pool = self.connection_pool.clone();
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
            SourcesMessage::Out(_) => {
                panic!("should be handled by the parent component")
            }
        }
    }

    fn view_source<'a>(&self, source: &'a Remote) -> Element<'a, SourcesMessage> {
        let cosmic_theme::Spacing { space_s, .. } = theme::active().cosmic().spacing;

        Row::with_children([
            icon::from_name("network-server-symbolic")
                .size(24)
                .apply(container)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Center)
                .width(Length::FillPortion(1))
                .into(),
            widget::text(&source.base_url)
                .width(Length::FillPortion(10))
                .into(),
            widget::button::icon(icon::from_name("edit-delete-symbolic").size(24))
                .on_press(SourcesMessage::RequestDeleteSource(source.clone()))
                .apply(container)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Center)
                .width(Length::FillPortion(1))
                .into(),
        ])
        .padding([0, space_s])
        .spacing(space_s)
        .align_y(Vertical::Center)
        .into()
    }
}
