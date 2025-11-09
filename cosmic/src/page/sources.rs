use archive_organizer::Builder;
use archive_organizer::api::{FileDataSource, Status};
use archive_organizer::client::FilesClient;
use archive_organizer::db::ConnectionPool;
use archive_organizer::db::dao::RemoteDao;
use archive_organizer::db::models::{NewRemote, Remote};
use cosmic::iced::Length;
use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::widget::{column, container, icon, row};
use cosmic::{Action, widget};
use cosmic::{Apply, Element, Task};
use cosmic::{cosmic_theme, task, theme};
use url::Url;

use crate::app::ContextView;
use crate::fl;
use crate::state::LoadedState;

pub type SourcesState = LoadedState<Vec<Remote>>;
pub type UrlState = LoadedState<Status>;

pub struct SourcesPage {
    connection_pool: ConnectionPool,
    sources_state: SourcesState,
    entered_url: String,
    entered_url_id: widget::Id, // Unique ID for focus management
    url_state: UrlState,
}

#[derive(Debug, Clone)]
pub enum SourcesOutput {}

#[derive(Debug, Clone)]
pub enum SourcesMessage {
    LoadSources,
    Loaded(Vec<Remote>),
    LoadingFailed(String),

    UpdateEnteredUrl(String),
    TestEnteredUrl { url: Url, do_submit: bool },
    InvalidUrl,
    UrlTestResult(Result<Status, String>),
    AddSource(String),
    SubmitSource(Url),
    SubmittedSource,

    Out(SourcesOutput),
}

impl SourcesPage {
    pub fn new(connection_pool: ConnectionPool) -> (Self, Task<Action<SourcesMessage>>) {
        (
            Self {
                connection_pool,
                sources_state: Default::default(),
                entered_url: Default::default(),
                entered_url_id: widget::Id::unique(),
                url_state: Default::default(),
            },
            task::message(SourcesMessage::LoadSources),
        )
    }

    pub fn view(&self) -> Element<'_, SourcesMessage> {
        let cosmic_theme::Spacing { space_s, .. } = theme::active().cosmic().spacing;

        let mut col = column();

        col = match &self.sources_state {
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
                            .apply_if(self.url_state.is_loaded(), |input| {
                                input.on_submit(SourcesMessage::AddSource)
                            })
                            .width(Length::FillPortion(10)),
                    )
                    .apply_if(matches!(self.url_state, LoadedState::Failed(_)), |col| {
                        let LoadedState::Failed(ref error) = self.url_state else {
                            unreachable!()
                        };
                        col.push(widget::text(fl!("generic-error", error = error.as_str())))
                    }),
            )
            .push(
                match self.url_state {
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
        match message {
            SourcesMessage::LoadSources => {
                let connection_pool = self.connection_pool.clone();
                task::future(async move {
                    match connection_pool.select_all_remotes() {
                        Ok(remotes) => SourcesMessage::Loaded(remotes),
                        Err(error) => SourcesMessage::LoadingFailed(format!("{error}")),
                    }
                })
            }
            SourcesMessage::Loaded(remotes) => {
                self.sources_state = SourcesState::Loaded(remotes);
                task::none()
            }
            SourcesMessage::LoadingFailed(error) => {
                self.sources_state = SourcesState::Failed(error);
                task::none()
            }
            SourcesMessage::UpdateEnteredUrl(url) => {
                self.entered_url = url;
                match self.entered_url.parse::<Url>() {
                    Ok(url) => task::message(SourcesMessage::TestEnteredUrl {
                        url,
                        do_submit: false,
                    }),
                    Err(_) => task::message(SourcesMessage::InvalidUrl),
                }
            }
            SourcesMessage::InvalidUrl => {
                self.url_state = UrlState::Failed(String::from("invalid-url"));
                widget::text_input::focus(self.entered_url_id.clone())
            }
            SourcesMessage::TestEnteredUrl { url, do_submit } => {
                self.url_state = UrlState::Loading;
                let client = FilesClient::new(url.clone()).expect("valid url");
                Task::batch(vec![
                    widget::text_input::focus(self.entered_url_id.clone()),
                    task::future(async move {
                        let result = client.status().await.map_err(|error| format!("{error}"));
                        match result {
                            Ok(_status) if do_submit => SourcesMessage::SubmitSource(url),
                            result => SourcesMessage::UrlTestResult(result),
                        }
                    }),
                ])
            }
            SourcesMessage::UrlTestResult(result) => {
                match result {
                    Ok(status) => self.url_state = UrlState::Loaded(status),
                    Err(error) => self.url_state = UrlState::Failed(error),
                }
                widget::text_input::focus(self.entered_url_id.clone())
            }
            SourcesMessage::AddSource(url) => {
                self.entered_url = url;
                match self.entered_url.parse::<Url>() {
                    Ok(url) => task::message(SourcesMessage::TestEnteredUrl {
                        url,
                        do_submit: true,
                    }),
                    Err(_) => task::message(SourcesMessage::InvalidUrl),
                }
            }
            SourcesMessage::SubmitSource(url) => {
                let connection_pool = self.connection_pool.clone();
                task::future(async move {
                    match connection_pool.insert_remote(NewRemote {
                        base_url: url.to_string(),
                    }) {
                        Ok(_) => SourcesMessage::SubmittedSource,
                        Err(error) => SourcesMessage::UrlTestResult(Err(format!("{error}"))),
                    }
                })
            }
            SourcesMessage::SubmittedSource => {
                self.entered_url.clear();
                self.url_state = Default::default();
                task::message(SourcesMessage::LoadSources)
            }
            SourcesMessage::Out(_) => {
                panic!("should be handled by the parent component")
            }
        }
    }

    fn view_source<'a>(&self, source: &'a Remote) -> Element<'a, SourcesMessage> {
        let cosmic_theme::Spacing { space_s, .. } = theme::active().cosmic().spacing;

        row()
            .push(
                icon::from_name("network-server-symbolic")
                    .size(24)
                    .apply(container)
                    .align_x(Horizontal::Center)
                    .align_y(Vertical::Center)
                    .width(Length::FillPortion(1)),
            )
            .push(widget::text(&source.base_url).width(Length::FillPortion(11)))
            .padding([0, space_s])
            .spacing(space_s)
            .align_y(Vertical::Center)
            .into()
    }
}
