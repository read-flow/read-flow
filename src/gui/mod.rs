mod files_page;
mod welcome_page;

use iced::{
    border,
    widget::{self, button, column, container, row, scrollable, text, text_input},
    Element, Task, Theme,
};
use indexmap::IndexMap;
use url::Url;

use crate::{
    api::FileDataSource,
    client::{self, FilesClient},
    db::{
        dao::{self, RemoteDao},
        datasource::DbClient,
        models::{NewRemote, Remote},
        ConnectionPool,
    },
    ApplicationModule,
};

#[derive(Debug, thiserror::Error)]
#[error("invalid message")]
struct InvalidMessage(Message);

#[derive(Debug, Clone)]
enum Message {
    Files(files_page::Message),
    Welcome(welcome_page::Message),
    SwitchTab(CurrentTab),
    EditNewRemoteUrl(String),
    AddNewRemoteUrl,
    RemoteUrlAdded(Result<Remote, dao::Error>),
    RemoteUrlVerified(Result<String, client::Error>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CurrentTab {
    Welcome,
    LocalFiles,
    RemoteFiles(Url),
}

pub trait IdentifyTab {
    fn tab(&self) -> CurrentTab;
}

struct Tabs {
    current_tab: CurrentTab,
    welcome_page: welcome_page::Page,
    local_files: files_page::Page<DbClient>,
    remote_files: IndexMap<Url, files_page::Page<FilesClient>>,
}

impl Tabs {
    fn new(application_module: ApplicationModule) -> Self {
        // TODO: proper error handling of unwraps here
        let remote_files = application_module
            .connection_pool
            .select_all_remotes()
            .unwrap()
            .into_iter()
            .map(|remote| {
                let remote_connection: Url = remote.base_url.parse().unwrap();
                let page =
                    files_page::Page::new(FilesClient::new(remote_connection.clone()).unwrap());
                (remote_connection, page)
            })
            .collect();

        Self {
            current_tab: CurrentTab::Welcome,
            local_files: files_page::Page::new(application_module.db_client()),
            welcome_page: welcome_page::Page::new(application_module),
            remote_files,
        }
    }

    fn init(&self) -> Task<Message> {
        let mut init_tasks = vec![];
        init_tasks.push(self.welcome_page.init());
        init_tasks.push(self.local_files.init());
        init_tasks.extend(self.remote_files.values().map(|tab| tab.init()));
        Task::batch(init_tasks)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Welcome(message) => self.welcome_page.update(message),
            Message::Files(message) => match message.tab() {
                CurrentTab::LocalFiles => self.local_files.update(message),
                CurrentTab::RemoteFiles(url) => self.remote_files[&url].update(message),
                _ => {
                    unreachable!("Not expected here: {message:?}")
                }
            },
            _ => panic!("Not expected here: {message:?}"),
        }
    }

    fn view(&self) -> Element<Message> {
        match &self.current_tab {
            CurrentTab::Welcome => self.welcome_page.view(),
            CurrentTab::LocalFiles => self.local_files.view(),
            CurrentTab::RemoteFiles(url) => self.remote_files[url].view(),
        }
    }
}

struct App {
    tabs: Tabs,
    connection_pool: ConnectionPool,
    new_remote_url: String,
}

impl App {
    fn new(application_module: ApplicationModule) -> (Self, Task<Message>) {
        let connection_pool = application_module.connection_pool.clone();
        let tabs = Tabs::new(application_module);
        let initialize_tabs = tabs.init();
        (
            App {
                tabs,
                connection_pool,
                new_remote_url: Default::default(),
            },
            initialize_tabs,
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SwitchTab(tab_page) => {
                self.tabs.current_tab = tab_page;
                Task::none()
            }
            Message::EditNewRemoteUrl(url) => {
                self.new_remote_url = url;
                Task::none()
            }
            Message::AddNewRemoteUrl => {
                let mut new_remote_url = Default::default();
                std::mem::swap(&mut new_remote_url, &mut self.new_remote_url);
                Task::perform(test_remote_url(new_remote_url.clone()), move |result| {
                    Message::RemoteUrlVerified(result.map(|_| new_remote_url.clone()))
                })
            }
            Message::RemoteUrlVerified(Ok(new_remote_url)) => Task::perform(
                add_remote_url(self.connection_pool.clone(), new_remote_url),
                Message::RemoteUrlAdded,
            ),
            Message::RemoteUrlVerified(Err(error)) => {
                tracing::error!("error while verifying remote: {error}");
                Task::none()
            }
            Message::RemoteUrlAdded(Ok(remote)) => {
                let base_url: Url = remote.base_url.parse().unwrap();
                let page = files_page::Page::new(FilesClient::new(base_url.clone()).unwrap());
                let initialize_page = page.init();
                self.tabs.remote_files.insert(base_url, page);
                initialize_page
            }
            Message::RemoteUrlAdded(Err(error)) => {
                tracing::error!("error while adding remote: {error}");
                Task::none()
            }
            Message::Welcome(_) | Message::Files(_) => self.tabs.update(message),
        }
    }

    fn view(&self) -> Element<Message> {
        let header_bar = row![container(text("ArchiveOrganizer"))];
        let mut side_bar = column![
            if matches!(self.tabs.current_tab, CurrentTab::Welcome) {
                row![button("Welcome").width(iced::Fill)]
            } else {
                row![button("Welcome")
                    .width(iced::Fill)
                    .on_press(Message::SwitchTab(CurrentTab::Welcome))]
            },
            if matches!(self.tabs.current_tab, CurrentTab::LocalFiles) {
                row![button("Local").width(iced::Fill)]
            } else {
                row![button("Local")
                    .width(iced::Fill)
                    .on_press(Message::SwitchTab(CurrentTab::LocalFiles))]
            }
        ];
        for remote_connection in self.tabs.remote_files.keys() {
            let mut button = button(column![
                row![text("Remote")],
                row![text(remote_connection.domain().unwrap()).size(11)],
            ])
            .width(iced::Fill);

            if !matches!(&self.tabs.current_tab, CurrentTab::RemoteFiles(url) if url == remote_connection)
            {
                button = button.on_press(Message::SwitchTab(CurrentTab::RemoteFiles(
                    remote_connection.clone(),
                )));
            }
            side_bar = side_bar.push(row![button]);
        }
        side_bar = side_bar.push(row![column![
            text_input("Remote URL", &self.new_remote_url).on_input(Message::EditNewRemoteUrl),
            button("Add remote").on_press(Message::AddNewRemoteUrl)
        ]]);

        let pane_content = self.tabs.view();
        layout(header_bar, side_bar, column![pane_content]).into()
    }
}

impl ApplicationModule {
    pub fn gui(self) -> iced::Result {
        iced::application("ArchiveOrganizer - Files", App::update, App::view)
            .theme(|_| Theme::TokyoNight)
            .run_with(|| App::new(self))
    }
}

fn tag_button(theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        border: border::rounded(8),
        ..button::secondary(theme, status)
    }
}

fn delete_tag_button(theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        border: border::rounded(8),
        ..button::danger(theme, status)
    }
}

fn layout<'a>(
    head: widget::Row<'a, Message>,
    bar: widget::Column<'a, Message>,
    main: widget::Column<'a, Message>,
) -> widget::Column<'a, Message> {
    //row![sidebar(bar), column![header(head), content(main)]]
    column![header(head), row![sidebar(bar), content(main)]]
}

fn header(row: widget::Row<Message>) -> widget::Container<'_, Message> {
    container(row.padding(10).align_y(iced::Center)).style(|theme| {
        let palette = theme.extended_palette();

        container::Style::default().border(border::color(palette.background.strong.color).width(1))
    })
}

fn sidebar(column: widget::Column<Message>) -> widget::Container<'_, Message> {
    let column = column.push(widget::Row::new().height(iced::Fill));
    container(column.spacing(5).padding(10).width(200).align_x(iced::Left))
        .style(container::rounded_box)
        .center_y(iced::Fill)
}

fn content(column: widget::Column<Message>) -> widget::Container<'_, Message> {
    container(
        scrollable(column.spacing(40).align_x(iced::Left))
            .direction(scrollable::Direction::Both {
                vertical: scrollable::Scrollbar::new(),
                horizontal: scrollable::Scrollbar::new(),
            })
            .width(iced::Fill)
            .height(iced::Fill),
    )
    .padding(10)
}

async fn test_remote_url(base_url: String) -> Result<(), client::Error> {
    let client = FilesClient::new(base_url.parse::<Url>().unwrap()).unwrap();
    client.status().await?;
    Ok(())
}

async fn add_remote_url(
    connection_pool: ConnectionPool,
    base_url: String,
) -> Result<Remote, dao::Error> {
    tokio::task::block_in_place(|| connection_pool.insert_remote(NewRemote { base_url }))
}
