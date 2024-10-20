mod files_page;
mod welcome_page;

use iced::{
    border,
    widget::{self, button, column, container, row, scrollable, text},
    Element, Task, Theme,
};

use crate::db::ConnectionPool;

#[derive(Debug, thiserror::Error)]
#[error("invalid message")]
struct InvalidMessage(Message);

#[derive(Debug, Clone)]
enum Message {
    Files(files_page::Message),
    Welcome(welcome_page::Message),
    SwitchPage(Pages),
    InvalidMessage(Box<Message>),
}

#[derive(Debug, Clone)]
enum Pages {
    Files(files_page::Page),
    Welcome(welcome_page::Page),
}

impl Pages {
    fn init(&mut self) -> Task<Message> {
        match self {
            Pages::Files(ref mut page) => page.init(),
            Pages::Welcome(ref mut page) => page.init(),
        }
    }
}

struct App {
    page: Pages,
    connection_pool: ConnectionPool,
}

impl App {
    fn new(connection_pool: ConnectionPool) -> (Self, Task<Message>) {
        let page = welcome_page::Page::new(connection_pool.clone());
        let task = page.init();
        (
            App {
                page: page.into(),
                connection_pool,
            },
            task,
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        if let Message::InvalidMessage(message) = message {
            tracing::error!("invalid message: {message:?}");
            return Task::none();
        } else if let Message::SwitchPage(ref page) = message {
            self.page = page.clone();
            return self.page.init();
        };
        match &mut self.page {
            Pages::Files(ref mut page) => match files_page::Message::try_from(message) {
                Ok(message) => page.update(message),
                Err(InvalidMessage(message)) => {
                    Task::done(Message::InvalidMessage(Box::new(message)))
                }
            },
            Pages::Welcome(ref mut page) => match welcome_page::Message::try_from(message) {
                Ok(message) => page.update(message),
                Err(InvalidMessage(message)) => {
                    Task::done(Message::InvalidMessage(Box::new(message)))
                }
            },
        }
    }

    fn view(&self) -> Element<Message> {
        let header_bar = row![container(text("ArchiveOrganizer"))];
        let side_bar = column![
            if matches!(self.page, Pages::Welcome(_)) {
                row![button("Welcome").width(iced::Fill)]
            } else {
                row![button("Welcome")
                    .width(iced::Fill)
                    .on_press(Message::SwitchPage(
                        welcome_page::Page::new(self.connection_pool.clone()).into()
                    ))]
            },
            if matches!(self.page, Pages::Files(_)) {
                row![button("Files").width(iced::Fill)]
            } else {
                row![button("Files")
                    .width(iced::Fill)
                    .on_press(Message::SwitchPage(
                        files_page::Page::new(self.connection_pool.clone()).into()
                    ))]
            }
        ];
        let page_content = match &self.page {
            Pages::Files(page) => page.view(),
            Pages::Welcome(page) => page.view(),
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
