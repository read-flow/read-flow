use std::collections::HashMap;
use std::sync::Arc;

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
use crate::forms::sources::add_source::AddSourceForm;
use crate::forms::sources::add_source::AddSourceFormMessage;
use crate::forms::sources::add_source::AddSourceFormOutput;
use crate::iter::find_with_next;
use crate::iter::find_with_previous;
use crate::layout::layout;
use crate::page::Page;
use crate::state::LoadedState;

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
    add_source_form: Option<AddSourceForm>,
    operation_error: Option<String>,
    pending_deletion: Option<Remote>,
    source_statuses: HashMap<i32, LoadedState<bool>>,
}

#[derive(Debug, Clone)]
pub enum SourcesOutput {
    AddedSource(Url, String, String),       // url, user_id, passphrase
    EditedSource(Url, Url, String, String), // old_url, new_url, user_id, passphrase
    DeletedSource(Url),
}

#[derive(Debug, Clone)]
pub enum SourcesMessage {
    Remotes(ProvidedStateMessage<Vec<Remote>>),

    ShowAddForm,
    EditSource(Remote),
    AddSourceForm(AddSourceFormMessage),

    SubmitSource(Option<Remote>, Url, String, String), // original (None=add), url, user_id, passphrase
    SubmittedSource(Option<Remote>, Url, String, String),
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

impl From<AddSourceFormMessage> for SourcesMessage {
    fn from(value: AddSourceFormMessage) -> Self {
        Self::AddSourceForm(value)
    }
}

impl SourcesPage {
    pub fn new(application_module: Arc<ApplicationModule>) -> (Self, Task<Action<SourcesMessage>>) {
        let (remotes_state, init_remotes_state) =
            ProvidedState::new(RemotesProvider(application_module.clone()));
        (
            Self {
                application_module,
                remotes_state,
                add_source_form: None,
                operation_error: None,
                pending_deletion: None,
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
                    widget::button::icon(icon::from_name("edit-symbolic").size(ICON_SIZE))
                        .class(theme::Button::Icon)
                        .on_press(SourcesMessage::EditSource(source.clone()))
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
                    .add(crate::component::section_helpers::section_add_button(
                        fl!("sources-add-button"),
                        self.add_source_form
                            .is_none()
                            .then_some(SourcesMessage::ShowAddForm),
                    ))
                    .into()
            }
        };

        content.push(sources_section);

        if let Some(form) = &self.add_source_form {
            content.push(form.view().map(SourcesMessage::AddSourceForm));
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
            return Some(crate::component::confirm_dialog::confirm_delete_dialog(
                fl!("sources-delete-confirm-title"),
                fl!("sources-delete-confirm-body"),
                &remote.base_url,
                fl!("sources-delete-confirm-delete"),
                fl!("sources-delete-confirm-cancel"),
                SourcesMessage::ConfirmDeleteSource,
                SourcesMessage::CancelDeleteSource,
            ));
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
                let (form, task) = AddSourceForm::new(None);
                self.add_source_form = Some(form);
                task.map(ActionExt::map_into)
            }
            SourcesMessage::EditSource(remote) => {
                let (form, task) = AddSourceForm::new(Some(&remote));
                self.add_source_form = Some(form);
                task.map(ActionExt::map_into)
            }
            SourcesMessage::AddSourceForm(msg) => match msg {
                AddSourceFormMessage::Out(output) => match output {
                    AddSourceFormOutput::Cancel => {
                        self.add_source_form = None;
                        task::none()
                    }
                    AddSourceFormOutput::Submit(original, url, user_id, passphrase) => {
                        task::message(SourcesMessage::SubmitSource(
                            original, url, user_id, passphrase,
                        ))
                    }
                },
                msg => match &mut self.add_source_form {
                    Some(form) => form.update(msg).map(ActionExt::map_into),
                    None => task::none(),
                },
            },
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
            SourcesMessage::SubmitSource(original, url, user_id, passphrase) => {
                let am = Arc::clone(&self.application_module);
                task::future(async move {
                    let connection_pool = am.connection_pool().await;
                    let mut conn = match connection_pool.acquire().await {
                        Ok(conn) => conn,
                        Err(e) => return SourcesMessage::SetOperationError(format!("{e}")),
                    };
                    match &original {
                        None => {
                            let order = match dao::select_all_remotes(&mut conn).await {
                                Ok(remotes) => remotes.len() + 1,
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
                                Ok(_) => SourcesMessage::SubmittedSource(
                                    original, url, user_id, passphrase,
                                ),
                                Err(error) => SourcesMessage::SetOperationError(format!("{error}")),
                            }
                        }
                        Some(existing) => {
                            match dao::update_remote(
                                &mut conn,
                                existing.id,
                                url.as_str(),
                                &user_id,
                                &passphrase,
                            )
                            .await
                            {
                                Ok(()) => SourcesMessage::SubmittedSource(
                                    original, url, user_id, passphrase,
                                ),
                                Err(error) => SourcesMessage::SetOperationError(format!("{error}")),
                            }
                        }
                    }
                })
            }
            SourcesMessage::SubmittedSource(original, url, user_id, passphrase) => {
                self.add_source_form = None;
                let output = match original {
                    None => SourcesOutput::AddedSource(url, user_id, passphrase),
                    Some(old) => SourcesOutput::EditedSource(
                        old.base_url.parse().unwrap(),
                        url,
                        user_id,
                        passphrase,
                    ),
                };
                task::message(SourcesMessage::Out(output)).chain(task::message(
                    SourcesMessage::Remotes(ProvidedStateMessage::Load),
                ))
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
                    .unwrap()
                    .iter()
                    .find(|a| a.id == id)
                    .unwrap();

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
                                FilesClient::new(url, remote.user_id, remote.passphrase, false)
                                    .unwrap();
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
            SourcesMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent component")
            }
        }
    }
}
