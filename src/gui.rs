use std::sync::Arc;

use diesel::prelude::*;
use iced::{
    widget::{
        button, center, column, container, mouse_area, opaque, row, scrollable,
        scrollable::{Direction, Scrollbar},
        stack, text, text_input,
    },
    Color, Element, Task,
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
            Dialog::FileTag { tag, .. } => container(column![
                row![text("Add tag")],
                row![text_input("tag", &tag.clone().unwrap_or("".to_string()))
                    .width(250)
                    .on_input(Message::TagChanged)],
                row![button("close").on_press(Message::CloseDialog)],
            ])
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
}

pub fn gui() -> iced::Result {
    iced::application("ArchiveOrganizer - Files", Files::update, Files::view).run_with(Files::new)
}

impl Files {
    fn new() -> (Self, Task<Message>) {
        let mut this: Self = Default::default();
        let ordering = this.ordering;
        let connection_pool = this.connection_pool();
        (
            this,
            Task::batch([Task::perform(
                query_files(connection_pool, ordering),
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
                query_files(self.connection_pool(), self.ordering),
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
                }) => Task::perform(
                    add_file_tag(self.connection_pool(), FileTag { file_id, tag }),
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
        }
    }

    fn view(&self) -> Element<Message> {
        let action_bar = container(
            row![
                button("Update").on_press(Message::Update),
                button("Toggle Short Path").on_press(Message::ToggleShortenPath)
            ]
            .spacing(10),
        );

        let mut grid = Grid::new()
            .push(grid_row![
                text("actions"),
                button("id").on_press(Message::OrderBy(OrderFilesBy::Id)),
                button("type").on_press(Message::OrderBy(OrderFilesBy::Type)),
                button("size").on_press(Message::OrderBy(OrderFilesBy::Size)),
                button("fingerprint").on_press(Message::OrderBy(OrderFilesBy::Fingerprint)),
                button("path").on_press(Message::OrderBy(OrderFilesBy::Path)),
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
                    .extend(tags.iter().map(|tag| button(text(tag.tag.clone())).into()))
                    .spacing(5),
            ]);
        }

        let table = scrollable(grid).direction(Direction::Both {
            vertical: Scrollbar::new(),
            horizontal: Scrollbar::new(),
        });

        // container(column![action_bar, table,]).into()
        let content = container(column![action_bar, table,]);

        match &self.dialog {
            Some(dialog) => modal(content, dialog.to_element(), Message::Update),
            None => content.into(),
        }
    }
}

fn modal<'a, Message>(
    base: impl Into<Element<'a, Message>>,
    content: impl Into<Element<'a, Message>>,
    on_blur: Message,
) -> Element<'a, Message>
where
    Message: Clone + 'a,
{
    stack![
        base.into(),
        opaque(
            mouse_area(center(opaque(content)).style(|_theme| {
                container::Style {
                    background: Some(
                        Color {
                            a: 0.8,
                            ..Color::BLACK
                        }
                        .into(),
                    ),
                    ..container::Style::default()
                }
            }))
            .on_press(on_blur)
        )
    ]
    .into()
}

async fn query_files(
    connection_pool: ConnectionPool,
    order_by: OrderFilesBy,
) -> Result<Vec<(File, Vec<FileTag>)>, Error> {
    let files: Vec<File> = match order_by {
        OrderFilesBy::Id => files::dsl::files
            .order_by(files::columns::id)
            .load(&mut connection_pool.get().unwrap())?,
        OrderFilesBy::Type => files::dsl::files
            .order_by(files::columns::type_)
            .load(&mut connection_pool.get().unwrap())?,
        OrderFilesBy::Path => files::dsl::files
            .order_by(files::columns::path)
            .load(&mut connection_pool.get().unwrap())?,
        OrderFilesBy::Size => files::dsl::files
            .order_by(files::columns::size)
            .load(&mut connection_pool.get().unwrap())?,
        OrderFilesBy::Fingerprint => files::dsl::files
            .order_by(files::columns::sha256sum)
            .load(&mut connection_pool.get().unwrap())?,
    };

    let file_tags: Vec<FileTag> =
        file_tags::dsl::file_tags.load(&mut connection_pool.get().unwrap())?;

    let mut result: IndexMap<i32, (File, Vec<FileTag>)> = files
        .into_iter()
        .map(|file| (file.id, (file, Vec::new())))
        .collect();

    for tag in file_tags {
        if let Some((_file, tags)) = result.get_mut(&tag.file_id) {
            tags.push(tag);
        }
    }
    Ok(result.into_values().collect())
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
