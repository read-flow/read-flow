mod files_page;

use iced::{
    border,
    widget::{self, button, column, container, row, scrollable, text},
    Element, Task, Theme,
};

#[derive(Debug, Clone)]
enum Message {
    Files(files_page::Message),
}

trait Page {
    fn update(&mut self, message: Message) -> Task<Message>;
    fn view(&self) -> Element<Message>;
}

struct App {
    page: Box<dyn Page>,
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let (page, task) = files_page::Page::new();
        (
            App {
                page: Box::new(page),
            },
            task,
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        self.page.update(message)
    }

    fn view(&self) -> Element<Message> {
        let page_content = self.page.view();
        layout(row![], column![], column![page_content]).into()
    }
}

pub fn gui() -> iced::Result {
    iced::application("ArchiveOrganizer - Files", App::update, App::view)
        .theme(|_| Theme::Nord)
        .run_with(App::new)
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
    column![header(head), row![sidebar(bar), content(main)]]
}

fn header(row: widget::Row<Message>) -> widget::Container<'_, Message> {
    container(row.padding(10).align_y(iced::Center)).style(|theme| {
        let palette = theme.extended_palette();

        container::Style::default().border(border::color(palette.background.strong.color).width(1))
    })
}

fn sidebar(column: widget::Column<Message>) -> widget::Container<'_, Message> {
    container(
        column
            .spacing(40)
            .padding(10)
            .width(200)
            .align_x(iced::Center),
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
