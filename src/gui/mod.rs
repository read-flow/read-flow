mod files_page;
mod welcome_page;

use iced::{
    border,
    widget::{self, button, column, container, row, scrollable, text, text_input},
    Element, Task, Theme,
};
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
};

#[derive(Debug, thiserror::Error)]
#[error("invalid message")]
struct InvalidMessage(Message);

#[derive(Debug, Clone)]
enum Message {
    Files(files_page::Message),
    Welcome(welcome_page::Message),
    SwitchTab(TabPage),
    EditNewRemoteUrl(String),
    AddNewRemoteUrl,
    RemoteUrlAdded(Result<Remote, dao::Error>),
    RemoteUrlVerified(Result<String, client::Error>),
    RemoteUrlsListed(Result<Vec<Remote>, dao::Error>),
}

#[derive(Clone)]
enum TabPage {
    Welcome(welcome_page::Page),
    LocalFiles(files_page::Page<DbClient>),
    RemoteFiles(files_page::Page<FilesClient>, Url),
}

impl std::fmt::Debug for TabPage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TabPage::Welcome(_) => f.write_str("Pages::Welcome"),
            TabPage::LocalFiles(_) => f.write_str("Pages::LocalFiles"),
            TabPage::RemoteFiles(..) => f.write_str("Pages::RemoteFiles"),
        }
    }
}

impl TabPage {
    fn init(&mut self) -> Task<Message> {
        match self {
            TabPage::Welcome(ref mut page) => page.init(),
            TabPage::LocalFiles(ref mut page) => page.init(),
            TabPage::RemoteFiles(ref mut page, _) => page.init(),
        }
    }
}

impl From<welcome_page::Page> for TabPage {
    fn from(value: welcome_page::Page) -> Self {
        TabPage::Welcome(value)
    }
}

impl From<files_page::Page<DbClient>> for TabPage {
    fn from(source: files_page::Page<DbClient>) -> Self {
        TabPage::LocalFiles(source)
    }
}

struct App {
    current_tab: TabPage,
    connection_pool: ConnectionPool,
    remote_connections: Vec<Url>,
    new_remote_url: String,
}

impl App {
    fn new(connection_pool: ConnectionPool) -> (Self, Task<Message>) {
        let initial_tab = welcome_page::Page::new(connection_pool.clone());
        let initialize_tab = initial_tab.init();
        (
            App {
                current_tab: initial_tab.into(),
                connection_pool: connection_pool.clone(),
                remote_connections: Default::default(),
                new_remote_url: Default::default(),
            },
            Task::batch([
                initialize_tab,
                Task::perform(list_remote_urls(connection_pool), Message::RemoteUrlsListed),
            ]),
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SwitchTab(ref tab_page) => {
                self.current_tab = tab_page.clone();
                self.current_tab.init()
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
            Message::RemoteUrlAdded(Ok(remote)) => {
                let remote_url = remote.base_url.parse().unwrap();
                self.remote_connections.push(remote_url);
                Task::none()
            }
            Message::RemoteUrlVerified(Ok(new_remote_url)) => Task::perform(
                add_remote_url(self.connection_pool.clone(), new_remote_url),
                Message::RemoteUrlAdded,
            ),
            Message::RemoteUrlVerified(Err(error)) => {
                tracing::error!("error while verifying remote: {error}");
                Task::none()
            }
            Message::RemoteUrlAdded(Err(error)) => {
                tracing::error!("error while adding remote: {error}");
                Task::none()
            }
            Message::RemoteUrlsListed(Ok(urls)) => {
                let urls = urls
                    .into_iter()
                    .map(|url| url.base_url.parse().unwrap())
                    .collect();
                self.remote_connections = urls;
                Task::none()
            }
            Message::RemoteUrlsListed(Err(error)) => {
                tracing::error!("error while getting remote urls: {error}");
                Task::none()
            }
            Message::Welcome(welcome_message) => {
                if let TabPage::Welcome(ref mut current_tab) = self.current_tab {
                    current_tab.update(welcome_message)
                } else {
                    panic!("TODO return some error like 'page <TYPE> does not accept <MSG_TYPE> message");
                }
            }
            Message::Files(files_message) => {
                if let TabPage::LocalFiles(ref mut current_tab) = self.current_tab {
                    current_tab.update(files_message)
                } else if let TabPage::RemoteFiles(ref mut current_tab, _) = self.current_tab {
                    current_tab.update(files_message)
                } else {
                    panic!("TODO return some error like 'page <TYPE> does not accept <MSG_TYPE> message");
                }
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let header_bar = row![container(text("ArchiveOrganizer"))];
        let mut side_bar = column![
            if matches!(self.current_tab, TabPage::Welcome(_)) {
                row![button("Welcome").width(iced::Fill)]
            } else {
                row![button("Welcome")
                    .width(iced::Fill)
                    .on_press(Message::SwitchTab(
                        welcome_page::Page::new(self.connection_pool.clone()).into()
                    ))]
            },
            if matches!(self.current_tab, TabPage::LocalFiles(_)) {
                row![button("Local Files").width(iced::Fill)]
            } else {
                row![button("Local Files")
                    .width(iced::Fill)
                    .on_press(Message::SwitchTab(
                        files_page::Page::new(DbClient::new(self.connection_pool.clone())).into()
                    ))]
            }
        ];
        for remote_connection in self.remote_connections.iter() {
            let row = if matches!(&self.current_tab, TabPage::RemoteFiles(_, url) if url == remote_connection)
            {
                row![
                    button(text(format!("Remote Files\n({})", remote_connection)))
                        .width(iced::Fill)
                ]
            } else {
                row![
                    button(text(format!("Remote Files\n({})", remote_connection)))
                        .width(iced::Fill)
                        .on_press(Message::SwitchTab(TabPage::RemoteFiles(
                            files_page::Page::new(
                                FilesClient::new(remote_connection.clone()).unwrap()
                            ),
                            remote_connection.clone(),
                        )))
                ]
            };
            side_bar = side_bar.push(row);
        }
        side_bar = side_bar.push(row![column![
            text_input("Remote URL", &self.new_remote_url).on_input(Message::EditNewRemoteUrl),
            button("Add remote").on_press(Message::AddNewRemoteUrl)
        ]]);

        let pane_content = match &self.current_tab {
            TabPage::Welcome(tab) => tab.view(),
            TabPage::LocalFiles(tab) => tab.view(),
            TabPage::RemoteFiles(tab, _) => tab.view(),
        };
        layout(header_bar, side_bar, column![pane_content]).into()
    }
}

pub fn gui(connection_pool: ConnectionPool) -> iced::Result {
    iced::application("ArchiveOrganizer - Files", App::update, App::view)
        .theme(|_| Theme::Nord)
        .run_with(|| App::new(connection_pool))
}

fn tag_button(tag: String) -> button::Button<'static, Message> {
    button(text(tag).size(10)).style(|theme, status| button::Style {
        border: border::rounded(8),
        ..button::primary(theme, status)
    })
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
    container(
        column
            .spacing(40)
            .padding(10)
            .width(200)
            .align_x(iced::Left),
    )
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
    connection_pool.insert_remote(NewRemote { base_url })
}

async fn list_remote_urls(connection_pool: ConnectionPool) -> Result<Vec<Remote>, dao::Error> {
    connection_pool.select_all_remotes()
}
