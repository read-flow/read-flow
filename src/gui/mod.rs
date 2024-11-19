mod files_page;
mod welcome_page;

use iced::{
    border, padding,
    widget::{self, button, column, container, pick_list, row, scrollable, text, text_input},
    Element, Task, Theme,
};
use indexmap::{IndexMap, IndexSet};
use url::Url;

use crate::{
    api::{File, FileDataSource},
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
    // TODO: Add PageMessage
    Files(files_page::Message),
    Welcome(welcome_page::Message),
    SwitchTab(CurrentTab),
    EditNewRemoteUrl(String),
    AddNewRemoteUrl,
    RemoteUrlAdded(Result<Remote, dao::Error>),
    RemoteUrlVerified(Result<String, client::Error>),
    FindDuplicates(CurrentTab, String),
    Duplicates(CurrentTab, Vec<(CurrentTab, Vec<File>)>),
    GetTags(CurrentTab),
    ThemeSelected(Theme),
    Tags(CurrentTab, Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CurrentTabRef<'a> {
    Welcome,
    LocalFiles,
    RemoteFiles(&'a Url),
}

impl<'a> CurrentTabRef<'a> {
    fn button_text(&self) -> Element<'a, Message> {
        match self {
            Self::Welcome => text("Welcome").into(),
            Self::LocalFiles => text("Local").into(),
            Self::RemoteFiles(url) => {
                column![text("Remote"), text(url.domain().unwrap()).size(11),].into()
            }
        }
    }
}

impl<'a> From<&'a CurrentTab> for CurrentTabRef<'a> {
    fn from(value: &'a CurrentTab) -> Self {
        match value {
            CurrentTab::Welcome => Self::Welcome,
            CurrentTab::LocalFiles => Self::LocalFiles,
            CurrentTab::RemoteFiles(url) => Self::RemoteFiles(url),
        }
    }
}

impl<'a> From<CurrentTabRef<'a>> for CurrentTab {
    fn from(value: CurrentTabRef<'a>) -> Self {
        match value {
            CurrentTabRef::Welcome => Self::Welcome,
            CurrentTabRef::LocalFiles => Self::LocalFiles,
            CurrentTabRef::RemoteFiles(url) => Self::RemoteFiles(url.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CurrentTab {
    Welcome,
    LocalFiles,
    RemoteFiles(Url),
}

trait IdentifyTab {
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
            Message::Duplicates(tab, duplicates) => match tab {
                CurrentTab::LocalFiles => self.local_files.update((tab.clone(), duplicates).into()),
                CurrentTab::RemoteFiles(ref url) => {
                    self.remote_files[url].update((tab.clone(), duplicates).into())
                }
                _ => {
                    unreachable!()
                }
            },
            Message::Tags(tab, tags) => match tab {
                CurrentTab::LocalFiles => self.local_files.update((tab.clone(), tags).into()),
                CurrentTab::RemoteFiles(ref url) => {
                    self.remote_files[url].update((tab.clone(), tags).into())
                }
                _ => {
                    unreachable!()
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

    fn refresh_current_tab(&self) -> Task<Message> {
        match &self.current_tab {
            CurrentTab::Welcome => self.welcome_page.init(),
            CurrentTab::LocalFiles => self.local_files.init(),
            CurrentTab::RemoteFiles(url) => self.remote_files[url].init(),
        }
    }

    fn view_menu_entry<'a>(&'a self, tab: CurrentTabRef<'a>) -> Vec<Element<'a, Message>> {
        let button = button(tab.button_text()).width(iced::Fill);
        let mut side_bar = vec![];
        if tab == (&self.current_tab).into() {
            side_bar.push(button.into());
            side_bar.push(
                container(
                    widget::Column::new()
                        .spacing(5)
                        .extend(self.view_sub_menu()),
                )
                .padding(padding::left(10))
                .into(),
            );
        } else {
            side_bar.push(button.on_press(Message::SwitchTab(tab.into())).into());
        }
        side_bar
    }

    fn view_sub_menu(&self) -> Vec<Element<Message>> {
        match &self.current_tab {
            CurrentTab::Welcome => self.welcome_page.view_menu(),
            CurrentTab::LocalFiles => self.local_files.view_menu(),
            CurrentTab::RemoteFiles(url) => self.remote_files[url].view_menu(),
        }
    }

    fn duplicate_files(&self, fingerprint: &str) -> Vec<(CurrentTab, Vec<File>)> {
        let mut duplicates = Vec::new();
        duplicates.push((
            CurrentTab::LocalFiles,
            self.local_files.duplicate_files(fingerprint),
        ));
        duplicates.extend(self.remote_files.iter().map(|(remote, files)| {
            (
                CurrentTab::RemoteFiles(remote.clone()),
                files.duplicate_files(fingerprint),
            )
        }));
        duplicates
    }

    fn all_tags(&self) -> IndexSet<String> {
        let mut all_tags = IndexSet::new();
        all_tags.extend(self.local_files.all_tags());
        all_tags.extend(
            self.remote_files
                .values()
                .flat_map(files_page::Page::all_tags),
        );
        all_tags
    }
}

struct App {
    tabs: Tabs,
    connection_pool: ConnectionPool,
    new_remote_url: String,
    theme: Theme,
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
                theme: Theme::TokyoNight,
            },
            initialize_tabs,
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ThemeSelected(theme) => {
                self.theme = theme;
                Task::none()
            }
            Message::SwitchTab(tab) => {
                self.tabs.current_tab = tab.clone();
                self.tabs.refresh_current_tab()
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
            Message::Welcome(_)
            | Message::Files(_)
            | Message::Duplicates(..)
            | Message::Tags(..) => self.tabs.update(message),
            Message::FindDuplicates(tab, fingerprint) => Task::done(Message::Duplicates(
                tab,
                self.tabs.duplicate_files(&fingerprint),
            )),
            Message::GetTags(tab) => Task::done(Message::Tags(
                tab,
                self.tabs.all_tags().into_iter().collect(),
            )),
        }
    }

    fn view(&self) -> Element<Message> {
        let header_bar = row![container(text("ArchiveOrganizer"))];
        let mut side_bar = widget::Column::new();

        side_bar = side_bar
            .extend(self.tabs.view_menu_entry(CurrentTabRef::Welcome))
            .extend(self.tabs.view_menu_entry(CurrentTabRef::LocalFiles))
            .extend(self.tabs.remote_files.keys().flat_map(|remote| {
                self.tabs
                    .view_menu_entry(CurrentTabRef::RemoteFiles(remote))
            }))
            .push(container(""))
            .push(
                text_input("Remote URL", &self.new_remote_url)
                    .width(iced::Fill)
                    .on_input(Message::EditNewRemoteUrl),
            )
            .push(
                button("Add remote")
                    .width(iced::Fill)
                    .style(button::success)
                    .on_press(Message::AddNewRemoteUrl),
            )
            .push(container(""))
            .push(
                pick_list(Theme::ALL, Some(self.theme.clone()), Message::ThemeSelected)
                    .width(iced::Fill),
            )
            .spacing(5);

        let pane_content = self.tabs.view();
        let element: Element<Message> = layout(header_bar, side_bar, column![pane_content]).into();
        element
        // .explain(color!(0x0000ff))
    }
}

impl ApplicationModule {
    pub fn gui(self) -> iced::Result {
        iced::application("ArchiveOrganizer - Files", App::update, App::view)
            .theme(|app| app.theme.clone())
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

fn add_tag_button(theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        border: border::rounded(8),
        ..button::success(theme, status)
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
