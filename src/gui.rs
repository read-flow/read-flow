use std::sync::Arc;

use diesel::prelude::*;
use iced::{
    border,
    widget::{self, button, column, container, row, scrollable, text, text_input},
    Element, Task, Theme,
};
use iced_aw::{grid_row, Grid};
use indexmap::IndexMap;

use crate::{
    get_connection_pool,
    models::{File, FileTag},
    schema::{file_tags, files},
    ConnectionPool,
};

#[derive(Debug, Clone)]
enum Dialog {
    FileTag { file_id: i32, tag: Option<String> },
}

impl Dialog {
    fn file_tag(file_id: i32) -> Self {
        Dialog::FileTag { file_id, tag: None }
    }

    fn to_element(&self) -> Element<Message> {
        match self {
            Dialog::FileTag { tag, .. } => container(
                column![
                    row![text("Add tag")],
                    row![text_input("tag", &tag.clone().unwrap_or("".to_string()))
                        .width(250)
                        .on_input(Message::TagChanged)],
                    row![button("close").on_press(Message::CloseDialog)],
                ]
                .spacing(10),
            )
            .style(container::rounded_box)
            .padding(10),
        }
        .into()
    }
}

#[derive(Default)]
struct Files {
    shorten_path: bool,
    ordering: OrderFilesBy,
    connection_pool: Option<ConnectionPool>,
    files: Vec<(File, Vec<FileTag>)>,
    dialog: Option<Dialog>,
    selected_tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default)]
enum OrderFilesBy {
    #[default]
    Id,
    Type,
    Path,
    Size,
    Fingerprint,
}

#[derive(Debug, Clone, thiserror::Error)]
enum Error {
    #[error("database error: {0}")]
    DbError(#[source] Arc<diesel::result::Error>),
}

impl From<diesel::result::Error> for Error {
    fn from(value: diesel::result::Error) -> Self {
        Self::DbError(Arc::new(value))
    }
}

#[derive(Debug, Clone)]
enum Message {
    Update,
    ToggleShortenPath,
    CloseDialog,
    OpenDialog(Dialog),
    TagChanged(String),
    TagApplied(Result<FileTag, Error>),
    FilesLoaded(Result<Vec<(File, Vec<FileTag>)>, Error>),
    OrderBy(OrderFilesBy),
    AddTagFilter(String),
    RemoveTagFilter(String),
}

pub fn gui() -> iced::Result {
    iced::application("ArchiveOrganizer - Files", Files::update, Files::view)
        .theme(|_| Theme::Nord)
        .run_with(Files::new)
}

impl Files {
    fn new() -> (Self, Task<Message>) {
        let mut this: Self = Default::default();
        let ordering = this.ordering;
        let connection_pool = this.connection_pool();
        let selected_tags = this.selected_tags.clone();
        (
            this,
            Task::batch([Task::perform(
                query_files_by_tags(connection_pool, ordering, selected_tags),
                Message::FilesLoaded,
            )]),
        )
    }

    fn connection_pool(&mut self) -> ConnectionPool {
        if self.connection_pool.is_none() {
            self.connection_pool = Some(get_connection_pool());
        }
        // unwrap is safe because of previous code
        self.connection_pool.as_ref().unwrap().clone()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Update => Task::perform(
                query_files_by_tags(
                    self.connection_pool(),
                    self.ordering,
                    self.selected_tags.clone(),
                ),
                Message::FilesLoaded,
            ),
            Message::ToggleShortenPath => {
                self.shorten_path = !self.shorten_path;
                Task::none()
            }
            Message::CloseDialog => match self.dialog.take() {
                Some(Dialog::FileTag {
                    file_id,
                    tag: Some(tag),
                }) if !tag.trim().is_empty() => Task::perform(
                    add_file_tag(
                        self.connection_pool(),
                        FileTag {
                            file_id,
                            tag: tag.trim().to_string(),
                        },
                    ),
                    Message::TagApplied,
                ),
                _ => Task::none(),
            },
            Message::TagApplied(Ok(file_tag)) => {
                tracing::debug!("Added file_tag: {file_tag:?}");
                Task::done(Message::Update)
            }
            Message::TagApplied(Err(error)) => {
                tracing::error!("Could not add file_tag: {error}");
                Task::none()
            }
            Message::OpenDialog(dialog) => {
                self.dialog = Some(dialog);
                Task::none()
            }
            Message::TagChanged(tag) => {
                if let Some(Dialog::FileTag { file_id, .. }) = &self.dialog {
                    self.dialog = Some(Dialog::FileTag {
                        file_id: *file_id,
                        tag: Some(tag),
                    })
                }
                Task::none()
            }
            Message::FilesLoaded(Ok(files)) => {
                self.files = files;
                Task::none()
            }
            Message::FilesLoaded(Err(error)) => {
                tracing::error!("error while loading files from database: {error}");
                Task::none()
            }
            Message::OrderBy(ordering) => {
                self.ordering = ordering;
                Task::done(Message::Update)
            }
            Message::AddTagFilter(tag) => {
                self.selected_tags.push(tag);
                Task::done(Message::Update)
            }
            Message::RemoveTagFilter(tag) => {
                self.selected_tags.retain(|t| t != &tag);
                Task::done(Message::Update)
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let action_bar =
            row![button("Toggle Short Path").on_press(Message::ToggleShortenPath)].spacing(10);

        let mut grid = Grid::new()
            .push(grid_row![
                text("actions"),
                button("id").on_press(Message::OrderBy(OrderFilesBy::Id)),
                button("type").on_press(Message::OrderBy(OrderFilesBy::Type)),
                button("size").on_press(Message::OrderBy(OrderFilesBy::Size)),
                button("fingerprint").on_press(Message::OrderBy(OrderFilesBy::Fingerprint)),
                row![button("path").on_press(Message::OrderBy(OrderFilesBy::Path))]
                    .extend(self.selected_tags.iter().map(|t| {
                        tag_button(t.clone())
                            .on_press(Message::RemoveTagFilter(t.clone()))
                            .into()
                    }))
                    .spacing(5),
            ])
            .column_spacing(10);

        for (file, tags) in self.files.iter() {
            let path = if self.shorten_path {
                file.path.clone().split('/').last().unwrap().to_string()
            } else {
                file.path.clone()
            };

            grid = grid.push(grid_row![
                row![button("tag").on_press(Message::OpenDialog(Dialog::file_tag(file.id))),],
                text(file.id),
                text(file.type_.clone()),
                text(file.size),
                text(format!("{}...", &file.sha256sum[..9])),
                row![text(path)]
                    .extend(tags.iter().map(|tag| {
                        if self.selected_tags.contains(&tag.tag) {
                            tag_button(tag.tag.clone()).into()
                        } else {
                            tag_button(tag.tag.clone())
                                .on_press(Message::AddTagFilter(tag.tag.clone()))
                                .into()
                        }
                    }))
                    .spacing(5),
            ]);
        }

        let content = match &self.dialog {
            Some(dialog) => dialog.to_element(),
            None => grid.into(),
        };

        layout(action_bar, column![], column![content]).into()
    }
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

async fn query_files_by_tags(
    connection_pool: ConnectionPool,
    order_by: OrderFilesBy,
    tags: Vec<String>,
) -> Result<Vec<(File, Vec<FileTag>)>, Error> {
    let mut connection = connection_pool.get().unwrap();

    let files = files::table;
    let files: Vec<File> = match order_by {
        OrderFilesBy::Id => files.order_by(files::columns::id).load(&mut connection)?,
        OrderFilesBy::Type => files
            .order_by(files::columns::type_)
            .load(&mut connection)?,
        OrderFilesBy::Path => files.order_by(files::columns::path).load(&mut connection)?,
        OrderFilesBy::Size => files.order_by(files::columns::size).load(&mut connection)?,
        OrderFilesBy::Fingerprint => files
            .order_by(files::columns::sha256sum)
            .load(&mut connection)?,
    };

    let file_tags: Vec<FileTag> = file_tags::table.load(&mut connection)?;

    let mut result: IndexMap<i32, (File, Vec<FileTag>)> = files
        .into_iter()
        .map(|file| (file.id, (file, Vec::new())))
        .collect();

    for tag in file_tags {
        if let Some((_file, tags)) = result.get_mut(&tag.file_id) {
            tags.push(tag);
        }
    }

    Ok(result
        .into_values()
        .filter(|(_file, file_tags)| {
            let file_tags = file_tags.iter().map(|t| t.tag.clone()).collect::<Vec<_>>();
            tags.iter().all(|tag| file_tags.contains(tag))
        })
        .collect())
}

async fn add_file_tag(
    connection_pool: ConnectionPool,
    file_tag: FileTag,
) -> Result<FileTag, Error> {
    let file_tag = diesel::insert_into(file_tags::table)
        .values(&file_tag)
        .returning(FileTag::as_returning())
        .get_result(&mut connection_pool.get().unwrap())?;
    Ok(file_tag)
}
