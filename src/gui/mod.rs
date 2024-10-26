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
    SwitchPage(Pages),
    EditNewRemoteUrl(String),
    AddNewRemoteUrl,
    RemoteUrlAdded(Result<Remote, dao::Error>),
    RemoteUrlVerified(Result<String, client::Error>),
    RemoteUrlsListed(Result<Vec<Remote>, dao::Error>),
}

#[derive(Clone)]
enum Pages {
    Welcome(welcome_page::Page),
    LocalFiles(files_page::Page<DbClient>),
    RemoteFiles(files_page::Page<FilesClient>, Url),
}

impl std::fmt::Debug for Pages {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Pages::Welcome(_) => f.write_str("Pages::Welcome"),
            Pages::LocalFiles(_) => f.write_str("Pages::LocalFiles"),
            Pages::RemoteFiles(..) => f.write_str("Pages::RemoteFiles"),
        }
    }
}

impl Pages {
    fn init(&mut self) -> Task<Message> {
        match self {
            Pages::Welcome(ref mut page) => page.init(),
            Pages::LocalFiles(ref mut page) => page.init(),
            Pages::RemoteFiles(ref mut page, _) => page.init(),
        }
    }
}

impl From<files_page::Page<DbClient>> for Pages {
    fn from(source: files_page::Page<DbClient>) -> Self {
        Pages::LocalFiles(source)
    }
}

struct App {
    page: Pages,
    connection_pool: ConnectionPool,
    remote_connections: Vec<Url>,
    new_remote_url: String,
}

impl App {
    fn new(connection_pool: ConnectionPool) -> (Self, Task<Message>) {
        let page = welcome_page::Page::new(connection_pool.clone());
        let task = page.init();
        (
            App {
                page: page.into(),
                connection_pool: connection_pool.clone(),
                remote_connections: Default::default(),
                new_remote_url: Default::default(),
            },
            Task::batch([
                task,
                Task::perform(list_remote_urls(connection_pool), Message::RemoteUrlsListed),
            ]),
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SwitchPage(ref page) => {
                self.page = page.clone();
                self.page.init()
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
                if let Pages::Welcome(ref mut page) = self.page {
                    page.update(welcome_message)
                } else {
                    panic!("TODO return some error like 'page <TYPE> does not accept <MSG_TYPE> message");
                }
            }
            Message::Files(files_message) => {
                if let Pages::LocalFiles(ref mut page) = self.page {
                    page.update(files_message)
                } else if let Pages::RemoteFiles(ref mut page, _) = self.page {
                    page.update(files_message)
                } else {
                    panic!("TODO return some error like 'page <TYPE> does not accept <MSG_TYPE> message");
                }
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let header_bar = row![container(text("ArchiveOrganizer"))];
        let mut side_bar = column![
            if matches!(self.page, Pages::Welcome(_)) {
                row![button("Welcome").width(iced::Fill)]
            } else {
                row![button("Welcome")
                    .width(iced::Fill)
                    .on_press(Message::SwitchPage(
                        welcome_page::Page::new(self.connection_pool.clone()).into()
                    ))]
            },
            if matches!(self.page, Pages::LocalFiles(_)) {
                row![button("Local Files").width(iced::Fill)]
            } else {
                row![button("Local Files")
                    .width(iced::Fill)
                    .on_press(Message::SwitchPage(
                        files_page::Page::new(DbClient::new(self.connection_pool.clone())).into()
                    ))]
            }
        ];
        for remote_connection in self.remote_connections.iter() {
            let row = if matches!(&self.page, Pages::RemoteFiles(_, url) if url == remote_connection)
            {
                row![
                    button(text(format!("Remote Files\n({})", remote_connection)))
                        .width(iced::Fill)
                ]
            } else {
                row![
                    button(text(format!("Remote Files\n({})", remote_connection)))
                        .width(iced::Fill)
                        .on_press(Message::SwitchPage(Pages::RemoteFiles(
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

        let page_content = match &self.page {
            Pages::Welcome(page) => page.view(),
            Pages::LocalFiles(page) => page.view(),
            Pages::RemoteFiles(page, _) => page.view(),
        };
        layout(header_bar, side_bar, column![page_content]).into()
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
