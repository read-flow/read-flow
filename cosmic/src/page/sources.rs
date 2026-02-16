use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use archive_organizer::Builder;
use archive_organizer::api::FileDataSource;
use archive_organizer::api::Status;
use archive_organizer::client::FilesClient;
use archive_organizer::db;
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
use cosmic::widget::icon;
use cosmic::widget::settings;
use provider::r#async::Provider;
use url::Url;

use crate::ApplicationModule;
use crate::ICON_SIZE;
use crate::component::provided_state::ProvidedState;
use crate::component::provided_state::ProvidedStateMessage;
use crate::cosmic_ext::ActionExt;
use crate::fl;
use crate::iter::find_with_next;
use crate::iter::find_with_previous;
use crate::layout::layout;
use crate::page::Page;
use crate::state::LoadedState;

pub type UrlVerificationState = LoadedState<Status>;

const DEBOUNCE_TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Debug, Clone)]
struct RemotesProvider(Arc<ApplicationModule>);

impl Provider<Vec<Remote>> for RemotesProvider {
    type Error = db::dao::Error;

    async fn provide(&self) -> Result<Vec<Remote>, Self::Error> {
        self.0.connection_pool().select_all_remotes()
    }
}

pub struct SourcesPage {
    application_module: Arc<ApplicationModule>,
    remotes_state: ProvidedState<RemotesProvider, Vec<Remote>>,
    entered_url: String,
    entered_url_id: widget::Id, // Unique ID for focus management
    entered_user_id: String,
    entered_user_id_id: widget::Id, // Unique ID for focus management
    entered_passphrase: String,
    entered_passphrase_id: widget::Id, // Unique ID for focus management
    show_passphrase: bool,
    url_verification_state: UrlVerificationState,
    operation_error: Option<String>,
    pending_deletion: Option<Remote>,
    // Debouncing state
    last_input_time: Instant,
}

#[derive(Debug, Clone)]
pub enum SourcesOutput {
    AddedSource(Url, String, String), // url, user_id, passphrase
    DeletedSource(Url),
}

#[derive(Debug, Clone)]
pub enum SourcesMessage {
    Remotes(ProvidedStateMessage<Vec<Remote>>),

    UpdateEnteredUrl(String),
    UpdateEnteredUserId(String),
    UpdateEnteredPassphrase(String),
    DebounceVerify(widget::Id),
    ToggleShowPassphrase,
    VerifyEnteredUrl {
        url: Url,
        user_id: String,
        passphrase: String,
        do_submit: bool,
        widget: widget::Id,
    },
    SetUrlVerificationStateFailed(widget::Id, String),
    SetUrlVerificationStateLoaded(widget::Id, Status),
    ClearUrlEntries,

    AddSource(String),
    SubmitSource(Url, String, String), // url, user_id, passphrase
    SubmittedSource(Url, String, String), // url, user_id, passphrase
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

impl From<ProvidedStateMessage<Vec<Remote>>> for SourcesMessage {
    fn from(value: ProvidedStateMessage<Vec<Remote>>) -> Self {
        Self::Remotes(value)
    }
}

impl SourcesPage {
    fn start_debounce_verification(
        &mut self,
        widget_id: widget::Id,
    ) -> Task<Action<SourcesMessage>> {
        self.last_input_time = Instant::now();
        // Start debounce task
        task::future(async move {
            tokio::time::sleep(DEBOUNCE_TIMEOUT).await;
            SourcesMessage::DebounceVerify(widget_id)
        })
    }

    pub fn new(application_module: Arc<ApplicationModule>) -> (Self, Task<Action<SourcesMessage>>) {
        let (remotes_state, init_remotes_state) =
            ProvidedState::new(RemotesProvider(application_module.clone()));
        (
            Self {
                application_module,
                remotes_state,
                entered_url: Default::default(),
                entered_url_id: widget::Id::unique(),
                entered_user_id: Default::default(),
                entered_user_id_id: widget::Id::unique(),
                entered_passphrase: Default::default(),
                entered_passphrase_id: widget::Id::unique(),
                show_passphrase: false,
                url_verification_state: Default::default(),
                operation_error: None,
                pending_deletion: None,
                last_input_time: Instant::now(),
            },
            task::batch([init_remotes_state.map(ActionExt::map_into)]),
        )
    }

    fn view_source<'a>(
        &self,
        source: &'a Remote,
        is_first: bool,
        is_last: bool,
    ) -> Element<'a, SourcesMessage> {
        widget::settings::item::builder(&source.base_url)
            .icon(icon::from_name("network-server-symbolic").size(ICON_SIZE))
            .control(widget::settings::item_row(vec![
                widget::button::icon(icon::from_name("go-up-symbolic").size(ICON_SIZE))
                    .class(theme::Button::Icon)
                    .apply_if(!is_first, |button| {
                        button.on_press(SourcesMessage::MoveSourceUp(source.clone()))
                    })
                    .into(),
                widget::button::icon(icon::from_name("go-down-symbolic").size(ICON_SIZE))
                    .class(theme::Button::Icon)
                    .apply_if(!is_last, |button| {
                        button.on_press(SourcesMessage::MoveSourceDown(source.clone()))
                    })
                    .into(),
                widget::button::icon(icon::from_name("list-remove-symbolic").size(ICON_SIZE))
                    .class(theme::Button::Destructive)
                    .on_press(SourcesMessage::RequestDeleteSource(source.clone()))
                    .into(),
            ]))
            .into()
    }

    fn verify_entered_url(&mut self, widget: widget::Id) -> Task<Action<SourcesMessage>> {
        self.url_verification_state = UrlVerificationState::New;
        if self.entered_url.is_empty()
            || self.entered_user_id.is_empty()
            || self.entered_passphrase.is_empty()
        {
            widget::text_input::focus(widget.clone())
        } else {
            match self.entered_url.parse::<Url>() {
                Ok(url) => task::message(SourcesMessage::VerifyEnteredUrl {
                    url,
                    user_id: self.entered_user_id.clone(),
                    passphrase: self.entered_passphrase.clone(),
                    do_submit: false,
                    widget: widget.clone(),
                }),
                Err(_) => task::message(SourcesMessage::SetUrlVerificationStateFailed(
                    widget.clone(),
                    fl!("sources-invalid-url"),
                )),
            }
        }
    }
}

impl Page for SourcesPage {
    type Message = SourcesMessage;

    fn view(&self) -> Element<'_, SourcesMessage> {
        let mut content = Vec::new();

        // Sources list section
        let sources_section = match &self.remotes_state.state {
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
            .add(
                widget::settings::item::builder(fl!("sources-url"))
                    .icon(icon::from_name("network-server-symbolic").size(ICON_SIZE))
                    .control(
                        widget::settings::item_row(vec![
                            widget::text_input(fl!("sources-url-placeholder"), &self.entered_url)
                                .id(self.entered_url_id.clone())
                                .on_input(SourcesMessage::UpdateEnteredUrl)
                                .width(Length::Fill)
                                .into(),
                            match self.url_verification_state {
                                LoadedState::New => icon::from_name("dialog-information-symbolic"),
                                LoadedState::Loading => icon::from_name("dialog-question-symbolic"),
                                LoadedState::Failed(_) => icon::from_name("dialog-error-symbolic"),
                                LoadedState::Loaded(_) => icon::from_name("emblem-ok-symbolic"),
                            }
                            .size(ICON_SIZE)
                            .into(),
                        ])
                        .width(Length::Fixed(600.0)),
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
                        .on_input(SourcesMessage::UpdateEnteredUserId)
                        .width(Length::Fixed(600.0)),
                    ),
            )
            .add(
                widget::settings::item::builder(fl!("sources-authorization-token"))
                    .icon(widget::icon::from_name("dialog-password-symbolic").size(ICON_SIZE))
                    .control(
                        widget::secure_input(
                            fl!("sources-authorization-token-placeholder"),
                            &self.entered_passphrase,
                            Some(SourcesMessage::ToggleShowPassphrase),
                            !self.show_passphrase,
                        )
                        .id(self.entered_passphrase_id.clone())
                        .on_input(SourcesMessage::UpdateEnteredPassphrase)
                        .width(Length::Fixed(600.0)),
                    ),
            )
            .add_maybe(
                matches!(self.url_verification_state, LoadedState::Failed(_)).then(|| {
                    let LoadedState::Failed(ref error) = self.url_verification_state else {
                        unreachable!()
                    };
                    widget::text::caption(error)
                }),
            )
            .add(widget::settings::item_row(vec![
                widget::horizontal_space().width(Length::Fill).into(),
                widget::button::suggested(fl!("sources-add-button"))
                    .apply_if(
                        !(self.entered_url.is_empty()
                            || self.entered_user_id.is_empty()
                            || self.entered_passphrase.is_empty()
                            || !matches!(self.url_verification_state, LoadedState::Loaded(_))),
                        |button| {
                            button.on_press(SourcesMessage::AddSource(self.entered_url.clone()))
                        },
                    )
                    .into(),
            ]));

        content.push(add_section.into());

        layout(settings::view_column(content))
            .apply(widget::scrollable::vertical)
            .apply(widget::container)
            .height(Length::Fill)
            .align_x(Horizontal::Center)
            .align_y(Vertical::Top)
            .into()
    }

    fn dialog(&self) -> Option<Element<'_, SourcesMessage>> {
        let cosmic_theme::Spacing { space_s, .. } = theme::active().cosmic().spacing;

        if let Some(remote) = &self.pending_deletion {
            return Some(
                widget::dialog()
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
                    )
                    .into(),
            );
        }

        if let Some(error) = &self.operation_error {
            return Some(
                widget::dialog()
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
                    )
                    .into(),
            );
        }

        None
    }

    fn update(&mut self, message: SourcesMessage) -> Task<Action<SourcesMessage>> {
        tracing::debug!("received: {message:?}");
        match message {
            SourcesMessage::Remotes(message) => {
                self.remotes_state.update(message).map(ActionExt::map_into)
            }
            SourcesMessage::UpdateEnteredUrl(url) => {
                self.entered_url = url;
                self.start_debounce_verification(self.entered_url_id.clone())
            }
            SourcesMessage::UpdateEnteredUserId(user_id) => {
                self.entered_user_id = user_id;
                self.start_debounce_verification(self.entered_user_id_id.clone())
            }
            SourcesMessage::UpdateEnteredPassphrase(passphrase) => {
                self.entered_passphrase = passphrase;
                self.start_debounce_verification(self.entered_passphrase_id.clone())
            }
            SourcesMessage::DebounceVerify(widget_id) => {
                // Only verify if enough time has passed since last input
                if self.last_input_time.elapsed() >= DEBOUNCE_TIMEOUT {
                    self.verify_entered_url(widget_id)
                } else {
                    task::none()
                }
            }
            SourcesMessage::SetUrlVerificationStateFailed(widget, error) => {
                self.url_verification_state = UrlVerificationState::Failed(error);
                widget::text_input::focus(widget)
            }
            SourcesMessage::VerifyEnteredUrl {
                url,
                user_id,
                passphrase,
                do_submit,
                widget,
            } => {
                self.url_verification_state = UrlVerificationState::Loading;
                let client = FilesClient::new(url.clone(), user_id.clone(), passphrase.clone())
                    .expect("valid url");
                Task::batch(vec![
                    widget::text_input::focus(widget.clone()),
                    task::future(async move {
                        match client.status().await {
                            Ok(_status) if do_submit => {
                                SourcesMessage::SubmitSource(url, user_id, passphrase)
                            }
                            Ok(status) => SourcesMessage::SetUrlVerificationStateLoaded(
                                widget.clone(),
                                status,
                            ),
                            Err(error) => SourcesMessage::SetUrlVerificationStateFailed(
                                widget,
                                format!("{error}"),
                            ),
                        }
                    }),
                ])
            }
            SourcesMessage::SetUrlVerificationStateLoaded(widget, status) => {
                self.url_verification_state = UrlVerificationState::Loaded(status);
                widget::text_input::focus(widget)
            }
            SourcesMessage::AddSource(url) => {
                self.entered_url = url;
                match self.entered_url.parse::<Url>() {
                    Ok(url) => task::message(SourcesMessage::VerifyEnteredUrl {
                        url,
                        user_id: self.entered_user_id.clone(),
                        passphrase: self.entered_passphrase.clone(),
                        do_submit: true,
                        widget: self.entered_url_id.clone(),
                    }),
                    Err(_) => task::message(SourcesMessage::SetUrlVerificationStateFailed(
                        self.entered_url_id.clone(),
                        String::from("invalid-url"),
                    )),
                }
            }
            SourcesMessage::SubmitSource(url, user_id, passphrase) => {
                let connection_pool = self.application_module.connection_pool();
                let order = self.remotes_state.state.unwrap().len() + 1;
                task::future(async move {
                    match connection_pool.insert_remote(NewRemote {
                        base_url: url.to_string(),
                        order: order as i32,
                        user_id: user_id.clone(),
                        passphrase: passphrase.clone(),
                    }) {
                        Ok(_) => SourcesMessage::SubmittedSource(url, user_id, passphrase),
                        Err(error) => SourcesMessage::SetOperationError(format!("{error}")),
                    }
                })
            }
            SourcesMessage::SubmittedSource(url, user_id, passphrase) => task::message(
                SourcesMessage::Out(SourcesOutput::AddedSource(url, user_id, passphrase)),
            )
            .chain(task::message(SourcesMessage::ClearUrlEntries)),
            SourcesMessage::ClearUrlEntries => {
                self.entered_url.clear();
                self.entered_user_id.clear();
                self.entered_passphrase.clear();
                self.url_verification_state = Default::default();
                task::message(SourcesMessage::Remotes(ProvidedStateMessage::Load))
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
                    .state
                    .unwrap() // should be safe, because otherwise `DeleteSource` message cannot be generated.
                    .iter()
                    .find(|a| a.id == id)
                    .unwrap(); // should be safe, because the source should exist.

                task::message(SourcesMessage::Out(SourcesOutput::DeletedSource(
                    remote.base_url.parse().unwrap(),
                )))
                .chain(task::message(SourcesMessage::Remotes(
                    ProvidedStateMessage::Load,
                )))
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
                find_with_previous(self.remotes_state.state.unwrap().iter(), |current| {
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
                find_with_next(self.remotes_state.state.unwrap().iter(), |current| {
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
                        Ok(_) => SourcesMessage::Remotes(ProvidedStateMessage::Load),
                        Err(error) => SourcesMessage::SetOperationError(format!("{error}")),
                    }
                })
            }
            SourcesMessage::ToggleShowPassphrase => {
                self.show_passphrase = !self.show_passphrase;
                task::none()
            }
            SourcesMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent component")
            }
        }
    }
}
