use diesel::prelude::*;
use iced::{
    widget::{
        button, center, column, container, mouse_area, opaque, row, scrollable,
        scrollable::{Direction, Scrollbar},
        stack, text, text_input,
    },
    Color, Element,
};
use iced_aw::{grid_row, Grid};

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
    connection_pool: Option<ConnectionPool>,
    files: Vec<File>,
    dialog: Option<Dialog>,
}

#[derive(Debug, Clone)]
enum Message {
    Update,
    ToggleShortenPath,
    CloseDialog,
    OpenDialog(Dialog),
    TagChanged(String),
    OrderById,
    OrderByType,
    OrderByPath,
    OrderBySize,
    OrderByFingerprint,
}

impl Files {
    fn connection_pool(&mut self) -> ConnectionPool {
        if self.connection_pool.is_none() {
            self.connection_pool = Some(get_connection_pool());
        }
        // unwrap is safe because of previous code
        self.connection_pool.as_ref().unwrap().clone()
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::Update => {
                self.files = files::dsl::files
                    .load(&mut self.connection_pool().get().unwrap())
                    .unwrap();
            }
            Message::ToggleShortenPath => {
                self.shorten_path = !self.shorten_path;
            }
            Message::CloseDialog => {
                if let Some(Dialog::FileTag {
                    file_id,
                    tag: Some(tag),
                }) = &self.dialog
                {
                    let file_tag = FileTag {
                        file_id: *file_id,
                        tag: tag.clone(),
                    };
                    diesel::insert_into(file_tags::table)
                        .values(&file_tag)
                        .returning(FileTag::as_returning())
                        .get_result(&mut self.connection_pool().get().unwrap())
                        .unwrap();
                }
                self.dialog = None;
            }
            Message::OpenDialog(dialog) => {
                self.dialog = Some(dialog);
            }
            Message::TagChanged(tag) => {
                if let Some(Dialog::FileTag { file_id, .. }) = &self.dialog {
                    self.dialog = Some(Dialog::FileTag {
                        file_id: *file_id,
                        tag: Some(tag),
                    })
                }
            }
            Message::OrderById => {
                self.files = files::dsl::files
                    .order_by(files::columns::id)
                    .load(&mut self.connection_pool().get().unwrap())
                    .unwrap();
            }
            Message::OrderByType => {
                self.files = files::dsl::files
                    .order_by(files::columns::type_)
                    .load(&mut self.connection_pool().get().unwrap())
                    .unwrap();
            }
            Message::OrderByPath => {
                self.files = files::dsl::files
                    .order_by(files::columns::path)
                    .load(&mut self.connection_pool().get().unwrap())
                    .unwrap();
            }
            Message::OrderBySize => {
                self.files = files::dsl::files
                    .order_by(files::columns::size)
                    .load(&mut self.connection_pool().get().unwrap())
                    .unwrap();
            }
            Message::OrderByFingerprint => {
                self.files = files::dsl::files
                    .order_by(files::columns::sha256sum)
                    .load(&mut self.connection_pool().get().unwrap())
                    .unwrap();
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
                button("id").on_press(Message::OrderById),
                button("type").on_press(Message::OrderByType),
                button("size").on_press(Message::OrderBySize),
                button("fingerprint").on_press(Message::OrderByFingerprint),
                button("path").on_press(Message::OrderByPath),
            ])
            .column_spacing(10);

        for file in self.files.iter() {
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
                text(path),
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

pub fn gui() -> iced::Result {
    iced::run("ArchiveOrganizer Files", Files::update, Files::view)
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
