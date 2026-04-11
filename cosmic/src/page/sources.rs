use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

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
use read_flow_core::Builder;
use read_flow_core::api::FileDataSource;
use read_flow_core::api::Status;
use read_flow_core::client::FilesClient;
use read_flow_core::db;
use read_flow_core::db::dao;
use read_flow_core::db::models::NewRemote;
use read_flow_core::db::models::Remote;
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
        let pool = self.0.connection_pool().await;
        let mut conn = pool.acquire().await?;
        dao::select_all_remotes(&mut conn).await
    }
}

pub struct SourcesPage {
    application_module: Arc<ApplicationModule>,
    remotes_state: ProvidedState<RemotesProvider, Vec<Remote>>,
    show_add_form: bool,
    entered_url: String,
    entered_url_id: widget::Id, // Unique ID for focus management
    entered_user_id: String,
    entered_user_id_id: widget::Id, // Unique ID for focus management
    entered_passphrase: String,
    entered_passphrase_id: widget::Id, // Unique ID for focus management
    show_passphrase: bool,
    url_verification_state: UrlVerificationState,
    unavailable_acknowledged: bool,
    operation_error: Option<String>,
    pending_deletion: Option<Remote>,
    // Debouncing state
    last_input_time: Instant,
    // Per-source reachability status (remote id → reachable)
    source_statuses: HashMap<i32, LoadedState<bool>>,
}

#[derive(Debug, Clone)]
pub enum SourcesOutput {
    AddedSource(Url, String, String), // url, user_id, passphrase
    DeletedSource(Url),
}

#[derive(Debug, Clone)]
pub enum SourcesMessage {
    Remotes(ProvidedStateMessage<Vec<Remote>>),

    ShowAddForm,
    CancelAddForm,

    UpdateEnteredUrl(String),
    UpdateEnteredUserId(String),
    UpdateEnteredPassphrase(String),
    DebounceVerify(widget::Id),
    ToggleShowPassphrase,
    ToggleUnavailableAcknowledged,
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

    CheckSourceStatus(Remote),
    SetSourceStatus(i32, bool),
    RefreshStatuses,

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
                show_add_form: false,
                entered_url: Default::default(),
                entered_url_id: widget::Id::unique(),
                entered_user_id: Default::default(),
                entered_user_id_id: widget::Id::unique(),
                entered_passphrase: Default::default(),
                entered_passphrase_id: widget::Id::unique(),
                show_passphrase: false,
                url_verification_state: Default::default(),
                unavailable_acknowledged: false,
                operation_error: None,
                pending_deletion: None,
                last_input_time: Instant::now(),
                source_statuses: HashMap::new(),
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
        let (status_icon_name, status_tooltip) = match self.source_statuses.get(&source.id) {
            None | Some(LoadedState::New) => {
                ("dialog-question-symbolic", fl!("sources-status-unknown"))
            }
            Some(LoadedState::Loading) => (
                "emblem-synchronizing-symbolic",
                fl!("sources-status-checking"),
            ),
            Some(LoadedState::Loaded(true)) => {
                ("emblem-ok-symbolic", fl!("sources-status-reachable"))
            }
            Some(LoadedState::Loaded(false)) | Some(LoadedState::Failed(_)) => (
                "network-offline-symbolic",
                fl!("sources-status-unreachable"),
            ),
        };
        let status_icon = widget::tooltip::tooltip(
            icon::from_name(status_icon_name).size(ICON_SIZE),
            widget::text(status_tooltip),
            widget::tooltip::Position::Bottom,
        );
        widget::settings::item::builder(&source.base_url)
            .icon(icon::from_name("network-server-symbolic").size(ICON_SIZE))
            .control(
                widget::settings::item_row(vec![
                    status_icon.into(),
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
                ])
                .width(Length::Shrink),
            )
            .into()
    }

    fn view_add_form(&self) -> Element<'_, SourcesMessage> {
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

        settings::section()
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
                        .on_input(SourcesMessage::UpdateEnteredUserId)
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
                            Some(SourcesMessage::ToggleShowPassphrase),
                            !self.show_passphrase,
                        )
                        .id(self.entered_passphrase_id.clone())
                        .on_input(SourcesMessage::UpdateEnteredPassphrase)
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
                            .on_toggle(|_| SourcesMessage::ToggleUnavailableAcknowledged),
                    )
            }))
            .add(widget::settings::item_row(vec![
                widget::space::horizontal().width(Length::Fill).into(),
                // Cancel button
                widget::button::icon(icon::from_name("edit-clear-all-symbolic").size(ICON_SIZE))
                    .on_press(SourcesMessage::CancelAddForm)
                    .into(),
                // Submit button
                widget::button::icon(icon::from_name("list-add-symbolic").size(ICON_SIZE))
                    .class(widget::button::ButtonClass::Suggested)
                    .apply_if(can_submit, |b| {
                        b.on_press(SourcesMessage::AddSource(self.entered_url.clone()))
                    })
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

        // Sources list section with add button at the bottom
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
                let section = if sources.is_empty() {
                    settings::section()
                        .title(fl!("sources-section-title"))
                        .add(widget::text(fl!("sources-empty-state")))
                } else {
                    sources.iter().enumerate().fold(
                        settings::section().title(fl!("sources-section-title")),
                        |section, (index, source)| {
                            section.add(self.view_source(
                                source,
                                index == 0,
                                index == sources.len() - 1,
                            ))
                        },
                    )
                };
                section
                    .add(widget::settings::item_row(vec![
                        widget::space::horizontal()
                            .width(Length::FillPortion(5))
                            .into(),
                        widget::button::icon(icon::from_name("list-add-symbolic").size(ICON_SIZE))
                            .class(widget::button::ButtonClass::Suggested)
                            .apply_if(!self.show_add_form, |b| {
                                b.on_press(SourcesMessage::ShowAddForm)
                            })
                            .tooltip(fl!("sources-add-button"))
                            .apply(widget::container)
                            .width(Length::FillPortion(1))
                            .align_x(Horizontal::Right)
                            .into(),
                    ]))
                    .into()
            }
        };

        content.push(sources_section);

        if self.show_add_form {
            content.push(self.view_add_form());
        }

        layout(settings::view_column(content))
            .apply(widget::scrollable::vertical)
            .width(Length::Fill)
            .height(Length::Fill)
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
            SourcesMessage::ShowAddForm => {
                self.show_add_form = true;
                widget::text_input::focus(self.entered_url_id.clone())
            }
            SourcesMessage::CancelAddForm => {
                self.show_add_form = false;
                self.entered_url.clear();
                self.entered_user_id.clear();
                self.entered_passphrase.clear();
                self.url_verification_state = Default::default();
                self.unavailable_acknowledged = false;
                task::none()
            }
            SourcesMessage::Remotes(message) => {
                let task = self.remotes_state.update(message).map(ActionExt::map_into);
                if let LoadedState::Loaded(remotes) = &self.remotes_state.state {
                    let mut check_tasks: Vec<_> = remotes
                        .iter()
                        .map(|remote| {
                            task::message(SourcesMessage::CheckSourceStatus(remote.clone()))
                        })
                        .collect();
                    check_tasks.push(task);
                    Task::batch(check_tasks)
                } else {
                    task
                }
            }
            SourcesMessage::UpdateEnteredUrl(url) => {
                self.entered_url = url;
                self.unavailable_acknowledged = false;
                self.start_debounce_verification(self.entered_url_id.clone())
            }
            SourcesMessage::UpdateEnteredUserId(user_id) => {
                self.entered_user_id = user_id;
                self.unavailable_acknowledged = false;
                self.start_debounce_verification(self.entered_user_id_id.clone())
            }
            SourcesMessage::UpdateEnteredPassphrase(passphrase) => {
                self.entered_passphrase = passphrase;
                self.unavailable_acknowledged = false;
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
                    Ok(url) => task::message(SourcesMessage::SubmitSource(
                        url,
                        self.entered_user_id.clone(),
                        self.entered_passphrase.clone(),
                    )),
                    Err(_) => task::message(SourcesMessage::SetUrlVerificationStateFailed(
                        self.entered_url_id.clone(),
                        fl!("sources-invalid-url"),
                    )),
                }
            }
            SourcesMessage::SubmitSource(url, user_id, passphrase) => {
                let am = Arc::clone(&self.application_module);
                let order = self.remotes_state.state.unwrap().len() + 1;
                task::future(async move {
                    let connection_pool = am.connection_pool().await;
                    let mut conn = match connection_pool.acquire().await {
                        Ok(conn) => conn,
                        Err(e) => return SourcesMessage::SetOperationError(format!("{e}")),
                    };
                    match dao::insert_remote(
                        &mut conn,
                        NewRemote {
                            base_url: url.to_string(),
                            order: order as i32,
                            user_id: user_id.clone(),
                            passphrase: passphrase.clone(),
                        },
                    )
                    .await
                    {
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
                self.show_add_form = false;
                self.entered_url.clear();
                self.entered_user_id.clear();
                self.entered_passphrase.clear();
                self.url_verification_state = Default::default();
                self.unavailable_acknowledged = false;
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
                let am = Arc::clone(&self.application_module);
                task::future(async move {
                    let connection_pool = am.connection_pool().await;
                    match dao::delete_remote_by_id(&connection_pool, id).await {
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
                let am = Arc::clone(&self.application_module);
                task::future(async move {
                    let connection_pool = am.connection_pool().await;
                    match dao::swap_order_of_remotes(&connection_pool, &first, &second).await {
                        Ok(_) => SourcesMessage::Remotes(ProvidedStateMessage::Load),
                        Err(error) => SourcesMessage::SetOperationError(format!("{error}")),
                    }
                })
            }
            SourcesMessage::RefreshStatuses => {
                if let LoadedState::Loaded(remotes) = &self.remotes_state.state {
                    let tasks: Vec<_> = remotes
                        .iter()
                        .map(|remote| {
                            task::message(SourcesMessage::CheckSourceStatus(remote.clone()))
                        })
                        .collect();
                    Task::batch(tasks)
                } else {
                    task::none()
                }
            }
            SourcesMessage::CheckSourceStatus(remote) => {
                self.source_statuses.insert(remote.id, LoadedState::Loading);
                task::future(async move {
                    let reachable = match remote.base_url.parse::<Url>() {
                        Ok(url) => {
                            let client =
                                FilesClient::new(url, remote.user_id, remote.passphrase).unwrap();
                            client.status().await.is_ok()
                        }
                        Err(_) => false,
                    };
                    SourcesMessage::SetSourceStatus(remote.id, reachable)
                })
            }
            SourcesMessage::SetSourceStatus(id, reachable) => {
                self.source_statuses
                    .insert(id, LoadedState::Loaded(reachable));
                task::none()
            }
            SourcesMessage::ToggleShowPassphrase => {
                self.show_passphrase = !self.show_passphrase;
                task::none()
            }
            SourcesMessage::ToggleUnavailableAcknowledged => {
                self.unavailable_acknowledged = !self.unavailable_acknowledged;
                task::none()
            }
            SourcesMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent component")
            }
        }
    }
}
