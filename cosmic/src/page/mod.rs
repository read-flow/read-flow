// SPDX-License-Identifier: GPL-3.0-or-later
mod files;

use crate::cosmic_ext::ActionExt;
use crate::fl;
use archive_organizer::ApplicationModule;
use archive_organizer::api::FileDataSource;
use archive_organizer::client::FilesClient;
use archive_organizer::db::dao::RemoteDao;
use archive_organizer::db::datasource::DbClient;
use cosmic::Element;
use cosmic::Task;
use cosmic::widget;
use files::Files;
use files::FilesMessage;
use url::Url;

pub struct Pages {
    local: Files<DbClient>,
    remotes: Vec<Files<FilesClient>>,
}

#[derive(Debug, Clone)]
pub enum PageSelector {
    LocalFiles,
    RemoteFiles(Url),
}

#[derive(Debug, Clone)]
pub enum PageMessage {
    LocalFiles(FilesMessage),
    RemoteFiles(Url, FilesMessage),
}

impl Pages {
    pub fn new(application_module: &ApplicationModule) -> Self {
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

        Self {
            local: Files::new(db_client),
            remotes: remote_clients.into_iter().map(Files::new).collect(),
        }
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
            PageSelector::LocalFiles => self.local.client.display_name(),
            PageSelector::RemoteFiles(base_url) => match self
                .remotes
                .iter()
                .find(|remote| remote.client.base_url == *base_url)
            {
                Some(remote) => remote.client.display_name(),
                None => fl!("unknown-remote", url = base_url.to_string()),
            },
        }
    }

    pub fn view<'a>(&'a self, active_page: &'a PageSelector) -> Element<'a, PageMessage> {
        match &active_page {
            PageSelector::LocalFiles => self.local.view().map(PageMessage::LocalFiles),
            PageSelector::RemoteFiles(base_url) => match self
                .remotes
                .iter()
                .find(|remote| remote.client.base_url == *base_url)
            {
                Some(remote) => remote
                    .view()
                    .map(|msg| PageMessage::RemoteFiles(remote.client.base_url.clone(), msg)),
                None => {
                    widget::text::title1(fl!("unknown-remote", url = base_url.to_string())).into()
                }
            },
        }
    }

    pub fn update(&mut self, message: PageMessage) -> Task<cosmic::Action<PageMessage>> {
        match message {
            PageMessage::LocalFiles(message) => self
                .local
                .update(message)
                .map(|action| action.map(PageMessage::LocalFiles)),
            PageMessage::RemoteFiles(base_url, message) => match self
                .remotes
                .iter_mut()
                .find(|files| files.client.base_url == base_url)
            {
                Some(ref mut remote) => remote.update(message).map(move |action| {
                    action.map(|msg| PageMessage::RemoteFiles(base_url.clone(), msg))
                }),
                None => Task::none(), // TODO: log
            },
        }
    }
}
