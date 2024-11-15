use std::sync::Arc;

use iced::{
    alignment::{Horizontal, Vertical},
    widget::{button, checkbox, container, row, text, text_input},
    Element, Task,
};
use iced_aw::{grid_row, Grid};
use indexmap::{IndexMap, IndexSet};
use regex::Regex;

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
    regex: Option<String>,
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
            regex: Default::default(),
        }
    }

    pub fn duplicate_files(&self, fingerprint: &str) -> Vec<File> {
        self.files
            .iter()
            .filter(|f| f.fingerprint == fingerprint)
            .cloned()
            .collect()
    }

    pub fn all_tags(&self) -> IndexSet<String> {
        self.files.iter().flat_map(|f| f.tags.clone()).collect()
    }

    pub fn init(&self) -> Task<gui::Message> {
        let ordering = self.ordering;
        let selected_tags = self.selected_tags.clone();
        let tab = self.tab();
        Task::perform(
            query_files_by_tags(
                self.file_data_source.clone(),
                ordering,
                selected_tags,
                self.regex.clone(),
            ),
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
                    self.regex.clone(),
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
            Message::OpenFile(tab, file) => {
                Task::perform(open_file(self.file_data_source.clone(), file), move |_| {
                    super::Message::Update(tab.clone()).into() // TODO: Should be noop
                })
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
            Message::SetRegex(tab, regex) => {
                if regex.is_empty() {
                    self.regex = None;
                } else {
                    self.regex = Some(regex);
                }
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
                checkbox("Short Path", self.shorten_path)
                    .width(iced::Fill)
                    .on_toggle(|_| Message::ToggleShortenPath(self.tab()).into())
                    .into(),
                checkbox("Duplicates", self.duplicates)
                    .width(iced::Fill)
                    .on_toggle(|_| Message::ToggleDuplicates(self.tab()).into())
                    .into(),
                text_input(
                    "Regular expression",
                    self.regex.as_ref().unwrap_or(&String::from("")),
                )
                .width(iced::Fill)
                .on_input(|value| Message::SetRegex(self.tab(), value).into())
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
    regex: Option<String>,
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

    let select_regex = regex.and_then(|r| Regex::new(&r).ok());

    Ok(files
        .into_iter()
        .filter(|file| tags.iter().all(|tag| file.tags.contains(tag)))
        .filter(|file| match &select_regex {
            Some(regex) => regex.is_match(&file.path),
            None => true,
        })
        .collect())
}

async fn open_file<FDS>(file_data_source: Arc<FDS>, file: File)
where
    FDS: FileDataSource,
    <FDS as FileDataSource>::Error: 'static,
{
    let result = file_data_source.xdg_open_file(file).await;
    match result {
        Ok(status) => tracing::info!("the command exited with: {status}"),
        Err(error) => tracing::error!("error while executing command: {error}"),
    }
}
