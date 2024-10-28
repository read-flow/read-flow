use std::sync::Arc;

use iced::{
    alignment::{Horizontal, Vertical},
    widget::{button, column, container, row, text, text_input},
    Element, Task,
};
use iced_aw::{grid_row, Grid};
use indexmap::IndexMap;

use crate::{
    api::{File, FileDataSource},
    client::FilesClient,
    gui::{self, CurrentTab, IdentifyTab},
    to_buckets,
};

use super::tag_button;

#[derive(Debug, Clone)]
pub(super) enum Dialog {
    FileTag {
        tab: CurrentTab,
        file_id: i32,
        tag: Option<String>,
    },
}

impl Dialog {
    fn file_tag(tab: CurrentTab, file_id: i32) -> Self {
        Dialog::FileTag {
            tab,
            file_id,
            tag: None,
        }
    }

    fn to_element(&self) -> Element<gui::Message> {
        match self {
            Dialog::FileTag { tab, tag, .. } => container(
                column![
                    row![text("Add tag")],
                    row![text_input("tag", &tag.clone().unwrap_or("".to_string()))
                        .width(250)
                        .on_input(|result| Message::TagChanged(tab.clone(), result).into())],
                    row![button("close").on_press(Message::CloseDialog(tab.clone()).into())],
                ]
                .spacing(10),
            )
            .style(container::rounded_box)
            .padding(10),
        }
        .into()
    }
}

#[derive(Debug, Clone)]
pub(super) struct Page<FDS> {
    shorten_path: bool,
    ordering: OrderFilesBy,
    file_data_source: Arc<FDS>,
    files: Vec<File>,
    dialog: Option<Dialog>,
    selected_tags: Vec<String>,
    duplicates: bool,
}

impl IdentifyTab for Page<gui::DbClient> {
    fn tab(&self) -> CurrentTab {
        CurrentTab::LocalFiles
    }
}

impl IdentifyTab for Page<FilesClient> {
    fn tab(&self) -> CurrentTab {
        CurrentTab::RemoteFiles(self.file_data_source.base_url().clone())
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) enum OrderFilesBy {
    #[default]
    Id,
    Type,
    Path,
    Size,
    Fingerprint,
}

#[derive(Debug, Clone, thiserror::Error)]
pub(super) enum Error {
    #[error("database error: {0}")]
    DataSourceError(String),
}

#[derive(Debug, Clone)]
pub(super) enum Message {
    Update(CurrentTab),
    ToggleShortenPath(CurrentTab),
    ToggleDuplicates(CurrentTab),
    CloseDialog(CurrentTab),
    OpenDialog(CurrentTab, Dialog),
    TagChanged(CurrentTab, String),
    TagApplied(CurrentTab, Result<Vec<String>, Error>),
    FilesLoaded(CurrentTab, Result<Vec<File>, Error>),
    OrderBy(CurrentTab, OrderFilesBy),
    AddTagFilter(CurrentTab, String),
    RemoveTagFilter(CurrentTab, String),
}

impl IdentifyTab for Message {
    fn tab(&self) -> CurrentTab {
        match self {
            Message::Update(tab) => tab.clone(),
            Message::ToggleShortenPath(tab) => tab.clone(),
            Message::ToggleDuplicates(tab) => tab.clone(),
            Message::CloseDialog(tab) => tab.clone(),
            Message::OpenDialog(tab, ..) => tab.clone(),
            Message::TagChanged(tab, ..) => tab.clone(),
            Message::TagApplied(tab, ..) => tab.clone(),
            Message::FilesLoaded(tab, ..) => tab.clone(),
            Message::OrderBy(tab, ..) => tab.clone(),
            Message::AddTagFilter(tab, ..) => tab.clone(),
            Message::RemoveTagFilter(tab, ..) => tab.clone(),
        }
    }
}

impl From<Message> for gui::Message {
    fn from(value: Message) -> Self {
        gui::Message::Files(value)
    }
}

impl TryFrom<gui::Message> for Message {
    type Error = gui::InvalidMessage;
    fn try_from(message: gui::Message) -> Result<Self, Self::Error> {
        if let gui::Message::Files(message) = message {
            Ok(message)
        } else {
            Err(gui::InvalidMessage(message))
        }
    }
}

impl<FDS> Page<FDS>
where
    FDS: FileDataSource + Send + Sync + 'static,
    Self: IdentifyTab,
{
    pub fn new(file_data_source: FDS) -> Self {
        Self {
            shorten_path: Default::default(),
            ordering: Default::default(),
            file_data_source: file_data_source.into(),
            files: Default::default(),
            dialog: Default::default(),
            selected_tags: Default::default(),
            duplicates: Default::default(),
        }
    }

    pub fn init(&self) -> Task<gui::Message> {
        let ordering = self.ordering;
        let selected_tags = self.selected_tags.clone();
        let tab = self.tab();
        Task::perform(
            query_files_by_tags(self.file_data_source.clone(), ordering, selected_tags),
            move |result| Message::FilesLoaded(tab.clone(), result).into(),
        )
    }

    pub fn update(&mut self, message: Message) -> Task<gui::Message> {
        match message {
            Message::Update(tab) => Task::perform(
                query_files_by_tags(
                    self.file_data_source.clone(),
                    self.ordering,
                    self.selected_tags.clone(),
                ),
                move |result| Message::FilesLoaded(tab.clone(), result).into(),
            ),
            Message::ToggleShortenPath(_) => {
                self.shorten_path = !self.shorten_path;
                Task::none()
            }
            Message::ToggleDuplicates(_) => {
                self.duplicates = !self.duplicates;
                Task::none()
            }
            Message::CloseDialog(_) => match self.dialog.take() {
                Some(Dialog::FileTag {
                    tab,
                    file_id,
                    tag: Some(tag),
                }) if !tag.trim().is_empty() => Task::perform(
                    add_file_tag(
                        self.file_data_source.clone(),
                        file_id,
                        tag.trim().to_string(),
                    ),
                    move |result| Message::TagApplied(tab.clone(), result).into(),
                ),
                _ => Task::none(),
            },
            Message::TagApplied(tab, Ok(file_tag)) => {
                tracing::debug!("Added file_tag: {file_tag:?}");
                Task::done(Message::Update(tab).into())
            }
            Message::TagApplied(_, Err(error)) => {
                tracing::error!("Could not add file_tag: {error}");
                Task::none()
            }
            Message::OpenDialog(_, dialog) => {
                self.dialog = Some(dialog);
                Task::none()
            }
            Message::TagChanged(tab, tag) => {
                if let Some(Dialog::FileTag { file_id, .. }) = &self.dialog {
                    self.dialog = Some(Dialog::FileTag {
                        tab,
                        file_id: *file_id,
                        tag: Some(tag),
                    })
                }
                Task::none()
            }
            Message::FilesLoaded(_, Ok(files)) => {
                self.files = files;
                Task::none()
            }
            Message::FilesLoaded(_, Err(error)) => {
                tracing::error!("error while loading files from database: {error}");
                Task::none()
            }
            Message::OrderBy(tab, ordering) => {
                self.ordering = ordering;
                Task::done(Message::Update(tab).into())
            }
            Message::AddTagFilter(tab, tag) => {
                self.selected_tags.push(tag);
                Task::done(Message::Update(tab).into())
            }
            Message::RemoveTagFilter(tab, tag) => {
                self.selected_tags.retain(|t| t != &tag);
                Task::done(Message::Update(tab).into())
            }
        }
    }

    pub fn view(&self) -> Element<gui::Message> {
        let action_bar = row![
            button("Toggle Short Path").on_press(Message::ToggleShortenPath(self.tab()).into()),
            button("Toggle Duplicates").on_press(Message::ToggleDuplicates(self.tab()).into()),
        ]
        .spacing(10);

        let mut grid = Grid::new()
            .push(grid_row![
                text("actions"),
                button("id").on_press(Message::OrderBy(self.tab(), OrderFilesBy::Id).into()),
                button("type").on_press(Message::OrderBy(self.tab(), OrderFilesBy::Type).into()),
                button("size").on_press(Message::OrderBy(self.tab(), OrderFilesBy::Size).into()),
                button("fingerprint")
                    .on_press(Message::OrderBy(self.tab(), OrderFilesBy::Fingerprint).into()),
                row![button("path")
                    .on_press(Message::OrderBy(self.tab(), OrderFilesBy::Path).into())]
                .extend(self.selected_tags.iter().map(|t| {
                    container(
                        tag_button(t.clone())
                            .on_press(Message::RemoveTagFilter(self.tab(), t.clone()).into()),
                    )
                    .padding(4)
                    .into()
                }))
                .spacing(5),
            ])
            .vertical_alignment(Vertical::Center)
            .horizontal_alignment(Horizontal::Left)
            .row_spacing(5)
            .column_spacing(10);

        let files: Vec<_> = if self.duplicates {
            let buckets: IndexMap<String, Vec<&File>> =
                to_buckets(self.files.iter(), |file| file.fingerprint.clone());
            buckets
                .into_iter()
                .filter(|(_, values)| values.len() > 1)
                .flat_map(|(_, values)| values)
                .collect()
        } else {
            self.files.iter().collect()
        };

        for file in files.iter() {
            let path = if self.shorten_path {
                file.path.clone().split('/').last().unwrap().to_string()
            } else {
                file.path.clone()
            };

            grid = grid.push(grid_row![
                row![button("tag").on_press(
                    Message::OpenDialog(self.tab(), Dialog::file_tag(self.tab(), file.id)).into()
                ),],
                text(file.id),
                text(file.type_.clone()),
                text(file.size),
                text(format!("{}...", &file.fingerprint[..9])),
                row![text(path)]
                    .extend(file.tags.iter().map(|tag| {
                        if self.selected_tags.contains(tag) {
                            tag_button(tag.clone()).into()
                        } else {
                            tag_button(tag.clone())
                                .on_press(Message::AddTagFilter(self.tab(), tag.clone()).into())
                                .into()
                        }
                    }))
                    .spacing(5),
            ]);
        }

        match &self.dialog {
            Some(dialog) => dialog.to_element(),
            None => column![action_bar, grid].spacing(10).into(),
        }
    }
}

async fn query_files_by_tags<FDS>(
    file_data_source: Arc<FDS>,
    order_by: OrderFilesBy,
    tags: Vec<String>,
) -> Result<Vec<File>, Error>
where
    FDS: FileDataSource,
    <FDS as FileDataSource>::Error: 'static,
{
    let mut files = file_data_source
        .get_files()
        .await
        .map_err(|error| Error::DataSourceError(format!("{error}")))?;

    match order_by {
        OrderFilesBy::Id => files.sort_by_key(|file| file.id),
        OrderFilesBy::Type => files.sort_by_key(|file| file.type_.clone()),
        OrderFilesBy::Path => files.sort_by_key(|file| file.path.clone()),
        OrderFilesBy::Size => files.sort_by_key(|file| file.size),
        OrderFilesBy::Fingerprint => files.sort_by_key(|file| file.fingerprint.clone()),
    };

    Ok(files
        .into_iter()
        .filter(|file| tags.iter().all(|tag| file.tags.contains(tag)))
        .collect())
}

async fn add_file_tag<FDS>(
    file_data_source: Arc<FDS>,
    file_id: i32,
    tag: String,
) -> Result<Vec<String>, Error>
where
    FDS: FileDataSource,
    <FDS as FileDataSource>::Error: 'static,
{
    let tags = file_data_source
        .add_file_tags(file_id, vec![tag])
        .await
        .map_err(|error| Error::DataSourceError(format!("{error}")))?;

    Ok(tags)
}
