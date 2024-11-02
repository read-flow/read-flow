use std::sync::Arc;

use iced::{
    alignment::{Horizontal, Vertical},
    widget::{button, container, row, text},
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

use super::{tag_button, Dialog, Error, Message, OrderFilesBy};

#[derive(Debug, Clone)]
pub struct Page<FDS> {
    shorten_path: bool,
    ordering: OrderFilesBy,
    file_data_source: Arc<FDS>,
    files: Vec<File>,
    dialog: Option<Dialog>,
    selected_tags: Vec<String>,
    duplicates: bool,
    is_offline: bool,
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
            is_offline: Default::default(),
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
            Message::Error(_, error) => {
                tracing::error!("error: {error}");
                Task::none()
            }
            Message::ToggleShortenPath(_) => {
                self.shorten_path = !self.shorten_path;
                Task::none()
            }
            Message::ToggleDuplicates(_) => {
                self.duplicates = !self.duplicates;
                Task::none()
            }
            Message::CancelDialog(_) => {
                self.dialog = None;
                Task::none()
            }
            Message::SubmitDialog(_) => match self.dialog.take() {
                Some(Dialog::EditFile(dialog)) => dialog.submit(self.file_data_source.clone()),
                None => Task::none(),
            },
            Message::OpenDialog(dialog) => {
                // task created here to avoid a clone
                let task = dialog.init();
                self.dialog = Some(dialog);
                task
            }
            Message::FilesLoaded(_, Ok(files)) => {
                self.files = files;
                Task::none()
            }
            Message::FilesLoaded(_, Err(error)) => {
                tracing::error!("error while loading files from database: {error}");
                self.is_offline = true;
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
            Message::EditDialog(message) => match &mut self.dialog {
                Some(Dialog::EditFile(ref mut dialog)) => dialog.update(message),
                None => Task::none(),
            },
        }
    }

    pub fn view_menu(&self) -> Vec<Element<gui::Message>> {
        if self.is_offline {
            vec![]
        } else {
            vec![
                button("Toggle Short Path")
                    .width(iced::Fill)
                    .on_press(Message::ToggleShortenPath(self.tab()).into())
                    .into(),
                button("Toggle Duplicates")
                    .width(iced::Fill)
                    .on_press(Message::ToggleDuplicates(self.tab()).into())
                    .into(),
            ]
        }
    }

    pub fn view(&self) -> Element<gui::Message> {
        if self.is_offline {
            return text("Offline").into();
        }

        // If a dialog is active, show that
        if let Some(dialog) = &self.dialog {
            return dialog.view();
        }

        let mut grid = Grid::new()
            .push(grid_row![
                button("id").on_press(Message::OrderBy(self.tab(), OrderFilesBy::Id).into()),
                button("type").on_press(Message::OrderBy(self.tab(), OrderFilesBy::Type).into()),
                button("size").on_press(Message::OrderBy(self.tab(), OrderFilesBy::Size).into()),
                button("fingerprint")
                    .on_press(Message::OrderBy(self.tab(), OrderFilesBy::Fingerprint).into()),
                row![button("path")
                    .on_press(Message::OrderBy(self.tab(), OrderFilesBy::Path).into())]
                .extend(self.selected_tags.iter().map(|t| {
                    container(
                        button(text(t).size(11))
                            .padding(4)
                            .style(tag_button)
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

        for file in files {
            let path = if self.shorten_path {
                file.path.clone().split('/').last().unwrap().to_string()
            } else {
                file.path.clone()
            };

            grid = grid.push(grid_row![
                text(file.id),
                text(file.type_.clone()),
                text(file.size),
                text(format!("{}...", &file.fingerprint[..9])),
                row![button(text(path)).style(button::text).on_press(
                    Message::OpenDialog(Dialog::edit_file(self.tab(), file.clone())).into()
                )]
                .extend(file.tags.iter().map(|tag| {
                    if self.selected_tags.contains(tag) {
                        button(text(tag).size(11))
                            .padding(4)
                            .style(tag_button)
                            .into()
                    } else {
                        button(text(tag).size(11))
                            .padding(4)
                            .style(tag_button)
                            .on_press(Message::AddTagFilter(self.tab(), tag.clone()).into())
                            .into()
                    }
                }))
                .spacing(5),
            ]);
        }

        grid.into()
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
