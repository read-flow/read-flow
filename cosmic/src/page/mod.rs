// SPDX-License-Identifier: GPL-3.0-or-later
mod file_details;
mod files;

use crate::cosmic_ext::ActionExt;
use crate::fl;
use archive_organizer::ApplicationModule;
use archive_organizer::api::File;
use archive_organizer::client::FilesClient;
use archive_organizer::db::dao::RemoteDao;
use archive_organizer::db::datasource::DbClient;
use cosmic::Element;
use cosmic::Task;
use cosmic::widget;
use file_details::FileDetails;
use file_details::FileDetailsMessage;
use files::Files;
use files::FilesMessage;
use files::FilesOutput;
use indexmap::IndexMap;
use rand::Rng;
use rand::rngs::ThreadRng;
use url::Url;

pub struct Pages {
    rng: ThreadRng,
    local: Files<DbClient>,
    remotes: Vec<Files<FilesClient>>,
    file_details: IndexMap<i32, FileDetails>,
}

#[derive(Debug, Clone)]
pub enum PageSelector {
    LocalFiles,
    RemoteFiles(Url),
    FileDetails(i32),
}

#[derive(Debug, Clone)]
pub enum PageOutput {
    PageAdded(PageSelector),
}

#[derive(Debug, Clone)]
pub enum PageMessage {
    LocalFiles(FilesMessage),
    RemoteFiles(Url, FilesMessage),
    FileDetails(i32, FileDetailsMessage),
    OpenFileDetails(File),
    CloseFileDetails(i32),
    Out(PageOutput),
}

impl Pages {
    pub fn new(
        application_module: &ApplicationModule,
    ) -> (Self, Task<cosmic::Action<PageMessage>>) {
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

        let (local, local_task) = Files::new(db_client);

        let mut tasks = vec![local_task.map(|action| action.map(PageMessage::LocalFiles))];

        let (remotes, remote_tasks): (_, Vec<Task<cosmic::Action<PageMessage>>>) = remote_clients
            .into_iter()
            .map(|remote_client| {
                let (remote, task) = Files::new(remote_client);
                let base_url = remote.client.base_url.clone();
                (
                    remote,
                    task.map(move |action| {
                        action.map(|msg| PageMessage::RemoteFiles(base_url.clone(), msg))
                    }),
                )
            })
            .unzip();

        tasks.extend(remote_tasks);

        (
            Self {
                rng: rand::rng(),
                local,
                remotes,
                file_details: Default::default(),
            },
            cosmic::task::batch(tasks),
        )
    }

    pub fn all_selectors(&self) -> Vec<PageSelector> {
        let mut selectors = vec![PageSelector::LocalFiles];
        for remote in &self.remotes {
            selectors.push(PageSelector::RemoteFiles(remote.client.base_url.clone()))
        }
        selectors
    }

    pub fn display_name<'a>(&'a self, page_selector: &'a PageSelector) -> String {
        match &page_selector {
            PageSelector::LocalFiles => self.local.display_name(),
            PageSelector::RemoteFiles(base_url) => match self
                .remotes
                .iter()
                .find(|remote| remote.client.base_url == *base_url)
            {
                Some(remote) => remote.display_name(),
                None => fl!("unknown-remote", url = base_url.to_string()),
            },
            PageSelector::FileDetails(id) => self.file_details[id].display_name(),
        }
    }

    pub fn view<'a>(&'a self, active_page: &'a PageSelector) -> Element<'a, PageMessage> {
        match &active_page {
            PageSelector::LocalFiles => self.local.view().map(map_local_files_message),
            PageSelector::RemoteFiles(base_url) => match self
                .remotes
                .iter()
                .find(|remote| remote.client.base_url == *base_url)
            {
                Some(remote) => remote
                    .view()
                    .map(|msg| map_remote_files_message(remote.client.base_url.clone(), msg)),
                None => {
                    widget::text::title1(fl!("unknown-remote", url = base_url.to_string())).into()
                }
            },
            PageSelector::FileDetails(id) => self.file_details[id]
                .view()
                .map(|msg| PageMessage::FileDetails(*id, msg)),
        }
    }

    pub fn update(&mut self, message: PageMessage) -> Task<cosmic::Action<PageMessage>> {
        match message {
            PageMessage::LocalFiles(message) => self
                .local
                .update(message)
                .map(|action| action.map(map_local_files_message)),
            PageMessage::RemoteFiles(base_url, message) => match self
                .remotes
                .iter_mut()
                .find(|files| files.client.base_url == base_url)
            {
                Some(ref mut remote) => remote.update(message).map(move |action| {
                    action.map(|msg| map_remote_files_message(base_url.clone(), msg))
                }),
                None => Task::none(), // TODO: log
            },
            PageMessage::FileDetails(id, message) => self.file_details[&id]
                .update(message)
                .map(move |action| action.map(|msg| PageMessage::FileDetails(id, msg))),
            PageMessage::OpenFileDetails(file) => {
                // TODO: only create new file_details if it does not yet exist
                let id = self.rng.random();
                let (file_details, initialization) = FileDetails::new(id, file);
                self.file_details.insert(id, file_details);
                let action = initialization
                    .map(move |action| action.map(|msg| PageMessage::FileDetails(id, msg)));
                action.chain(cosmic::task::message(PageMessage::Out(
                    PageOutput::PageAdded(PageSelector::FileDetails(id)),
                )))
            }
            PageMessage::CloseFileDetails(id) => {
                let _ = self.file_details.swap_remove(&id);
                // TODO: update App to change the active PageSelector
                cosmic::task::none()
            }
            PageMessage::Out(_) => {
                panic!("should be handled by the parent component")
            }
        }
    }
}

fn map_remote_files_message(base_url: Url, msg: FilesMessage) -> PageMessage {
    match msg {
        FilesMessage::Out(message) => match message {
            FilesOutput::OpenFileDetails(file) => PageMessage::OpenFileDetails(file),
        },
        msg => PageMessage::RemoteFiles(base_url, msg),
    }
}

fn map_local_files_message(msg: FilesMessage) -> PageMessage {
    match msg {
        FilesMessage::Out(message) => match message {
            FilesOutput::OpenFileDetails(file) => PageMessage::OpenFileDetails(file),
        },
        msg => PageMessage::LocalFiles(msg),
    }
}
