// SPDX-License-Identifier: GPL-3.0-or-later
// pages
mod file_details;
mod file_list;

// utilities
mod component;
mod state;

use core::panic;

use crate::app::ContextView;
use crate::client::ClientSelector;
use crate::cosmic_ext::ActionExt;
use crate::fl;
use archive_organizer::api::File;
use archive_organizer::client::FilesClient;
use archive_organizer::db::dao::RemoteDao;
use archive_organizer::ApplicationModule;
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::alignment::Vertical;
use cosmic::iced::Length;
use cosmic::task;
use cosmic::widget;
use cosmic::Action;
use cosmic::Apply;
use cosmic::Element;
use cosmic::Task;
use file_details::FileDetails;
use file_details::FileDetailsMessage;
use file_details::FileDetailsOutput;
use file_list::FileList;
use file_list::FileListMessage;
use file_list::FileListOutput;
use indexmap::IndexMap;
use rand::rngs::ThreadRng;
use rand::Rng;
use url::Url;

pub struct Pages {
    rng: ThreadRng,
    file_lists: IndexMap<ClientSelector, FileList>,
    file_details: IndexMap<i32, FileDetails>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PageSelector {
    FileList(ClientSelector),
    FileDetails(i32),
}

impl From<ClientSelector> for PageSelector {
    fn from(value: ClientSelector) -> Self {
        Self::FileList(value)
    }
}

#[derive(Debug, Clone)]
pub enum PageOutput {
    PageAdded(PageSelector),
    PageRemoved(PageSelector),
    ToggleContextPage(PageSelector),
}

#[derive(Debug, Clone)]
pub enum PageMessage {
    Files(ClientSelector, FileListMessage),
    FileDetails(i32, FileDetailsMessage),
    OpenFileDetails(ClientSelector, File),
    CloseFileDetails(i32),
    Out(PageOutput),
}

impl Pages {
    pub fn new(application_module: &ApplicationModule) -> (Self, Task<Action<PageMessage>>) {
        // Get the database client from the application module
        let db_client = application_module.db_client();

        // Get remote clients from the application module
        let remote_clients = application_module
            .connection_pool
            .select_all_remotes()
            .unwrap_or_default()
            .into_iter()
            .map(|remote| {
                let remote_connection: Url = remote.base_url.parse()?;
                let client = FilesClient::new(remote_connection.clone())?;
                Ok(client)
            })
            .collect::<anyhow::Result<_>>()
            .unwrap_or_else(|_e| {
                // tracing::error!("Failed to get remote clients: {}", e);
                Vec::new()
            });

        let (local, local_task) = FileList::new(db_client.into());

        let mut tasks = vec![local_task
            .map(|action| action.map(|msg| map_file_list_message(ClientSelector::Local, msg)))];

        let (mut remotes, remote_tasks): (Vec<FileList>, Vec<Task<Action<PageMessage>>>) =
            remote_clients
                .into_iter()
                .map(|remote_client| {
                    let base_url = remote_client.base_url.clone();
                    let (remote, task) = FileList::new(remote_client.into());
                    (
                        remote,
                        task.map(move |action| {
                            action.map(|msg| {
                                map_file_list_message(ClientSelector::Remote(base_url.clone()), msg)
                            })
                        }),
                    )
                })
                .unzip();

        tasks.extend(remote_tasks);

        let mut file_lists = vec![local];
        file_lists.append(&mut remotes);

        (
            Self {
                rng: rand::rng(),
                file_lists: file_lists
                    .into_iter()
                    .map(|file_list| (file_list.selector(), file_list))
                    .collect(),
                file_details: Default::default(),
            },
            task::batch(tasks),
        )
    }

    pub fn all_file_list_selectors(&self) -> Vec<PageSelector> {
        self.file_lists.keys().cloned().map(Into::into).collect()
    }

    pub fn display_name<'a>(&'a self, page_selector: &'a PageSelector) -> String {
        match &page_selector {
            PageSelector::FileList(selector) => self.file_lists[selector].display_name(),
            PageSelector::FileDetails(id) => self.file_details[id].display_name(),
        }
    }

    pub fn view<'a>(&'a self, active_page: &'a PageSelector) -> Element<'a, PageMessage> {
        match &active_page {
            PageSelector::FileList(selector) => self.file_lists[selector]
                .view()
                .map(|msg| map_file_list_message(selector.clone(), msg)),
            PageSelector::FileDetails(id) => self
                .file_details
                .get(id)
                .map(|page| page.view().map(|msg| map_file_details_message(*id, msg)))
                .unwrap_or_else(|| {
                    widget::text::title1(fl!("page-not-found"))
                        .apply(widget::container)
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .align_x(Horizontal::Center)
                        .align_y(Vertical::Center)
                        .into()
                }),
        }
    }

    pub fn view_context<'a>(
        &'a self,
        active_page: &'a PageSelector,
    ) -> ContextView<'a, PageMessage> {
        match &active_page {
            PageSelector::FileList(selector) => self.file_lists[selector]
                .view_context()
                .map(|msg| map_file_list_message(selector.clone(), msg)),
            PageSelector::FileDetails(id) => self
                .file_details
                .get(id)
                .map(|page| {
                    page.view_context()
                        .map(|msg| map_file_details_message(*id, msg))
                })
                .unwrap_or_else(|| ContextView {
                    title: fl!("page-not-found"),
                    content: widget::text::title1(fl!("page-not-found"))
                        .apply(widget::container)
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .align_x(Horizontal::Center)
                        .align_y(Vertical::Center)
                        .into(),
                }),
        }
    }

    pub fn update(&mut self, message: PageMessage) -> Task<Action<PageMessage>> {
        tracing::debug!("received: {message:?}");
        match message {
            PageMessage::Files(selector, message) => {
                match self.file_lists.get_mut(&selector) {
                    Some(page) => page.update(message).map(move |action| {
                        action.map(|msg| map_file_list_message(selector.clone(), msg))
                    }),
                    None => Task::none(), // TODO log
                }
            }
            PageMessage::FileDetails(id, message) => self.file_details[&id]
                .update(message)
                .map(move |action| action.map(|msg| map_file_details_message(id, msg))),
            PageMessage::OpenFileDetails(selector, file) => {
                // TODO: only create new file_details if it does not yet exist
                let id = self.rng.random();
                let file_list = &self.file_lists[&selector];
                let (file_details, initialization) =
                    FileDetails::new(id, file, file_list.client().clone());
                self.file_details.insert(id, file_details);
                let action = initialization
                    .map(move |action| action.map(|msg| map_file_details_message(id, msg)));
                action.chain(task::message(PageMessage::Out(PageOutput::PageAdded(
                    PageSelector::FileDetails(id),
                ))))
            }
            PageMessage::CloseFileDetails(id) => {
                let _ = self.file_details.swap_remove(&id);
                task::message(PageMessage::Out(PageOutput::PageRemoved(
                    PageSelector::FileDetails(id),
                )))
            }
            PageMessage::Out(_) => {
                panic!("should be handled by the parent component")
            }
        }
    }
}

fn map_file_list_message(selector: ClientSelector, msg: FileListMessage) -> PageMessage {
    match msg {
        FileListMessage::Out(message) => match message {
            FileListOutput::OpenFileDetails(file) => PageMessage::OpenFileDetails(selector, file),
            FileListOutput::ToggleContextPage(client_selector) => {
                PageMessage::Out(PageOutput::ToggleContextPage(client_selector.into()))
            }
        },
        msg => PageMessage::Files(selector, msg),
    }
}

fn map_file_details_message(id: i32, msg: FileDetailsMessage) -> PageMessage {
    match msg {
        FileDetailsMessage::Out(message) => match message {
            FileDetailsOutput::Close(id) => PageMessage::CloseFileDetails(id),
        },
        msg => PageMessage::FileDetails(id, msg),
    }
}
