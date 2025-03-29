use std::{cmp::Ordering, collections::HashSet, ffi::OsStr, path::Path, sync::Arc};

use iced::{
    Element, Task,
    widget::{
        Column, PickList, Row, button, checkbox, column, container, horizontal_rule, pick_list,
        row, text, text_input,
    },
};
use indexmap::{IndexMap, IndexSet};
use itertools::Itertools;
use regex::Regex;
use strum::IntoEnumIterator;

use crate::{
    Builder,
    api::{File, FileDataSource, ReadingStatus},
    client::FilesClient,
    gui::{self, CurrentTab, IdentifyTab, add_tag_button, delete_tag_button},
    settings::Settings,
    to_buckets,
};

use super::{Dialog, Error, Message, OrderDirection, OrderFilesBy, display_path, tag_button};

#[derive(Debug, Default, Clone)]
pub struct DisplayOptions {
    direction: OrderDirection,
    order: OrderFilesBy,
    shorten_path: bool,
}

#[derive(Debug, Default, Clone)]
pub struct FilterOptions {
    duplicates: bool,
    reading_status: HashSet<ReadingStatus>,
    regex: Option<String>,
    allow_tags: IndexSet<String>,
    deny_tags: IndexSet<String>,
}

#[derive(Debug, Clone)]
pub struct Page<FDS> {
    dialog: Option<Dialog>,
    display_options: DisplayOptions,
    file_data_source: Arc<FDS>,
    files: Vec<File>,
    filter_options: FilterOptions,
    is_offline: bool,
    selection_tag: Option<String>,
    settings: Arc<Settings>,
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
    pub fn new(settings: Arc<Settings>, file_data_source: FDS) -> Self {
        Self {
            display_options: DisplayOptions::default(),
            file_data_source: file_data_source.into(),
            files: Default::default(),
            dialog: Default::default(),
            filter_options: FilterOptions::default(),
            is_offline: Default::default(),
            selection_tag: Default::default(),
            settings,
        }
    }

    pub fn filtered_files(&self) -> Vec<&File> {
        filter_files(
            &self.files,
            &self.display_options,
            &self.filter_options,
            self.settings.clone(),
        )
    }

    pub fn duplicate_files(&self, fingerprint: &str) -> Vec<File> {
        self.filtered_files()
            .into_iter()
            .filter(|f| f.fingerprint == fingerprint)
            .cloned()
            .collect()
    }

    pub fn all_tags(&self) -> IndexSet<String> {
        self.files
            .iter()
            .flat_map(|f| f.tags.clone())
            .filter(|tag| !self.settings.ui.hidden_tags().contains(tag))
            .collect()
    }

    pub fn init(&self) -> Task<gui::Message> {
        Task::done(Message::LoadFiles(self.tab()).into())
    }

    pub fn update(&mut self, message: Message) -> Task<gui::Message> {
        match message {
            Message::LoadFiles(tab) => Task::perform(
                retrieve_files(self.file_data_source.clone()),
                move |result| Message::FilesLoaded(tab.clone(), result).into(),
            ),
            Message::Error(_, error) => {
                tracing::error!("error: {error}");
                Task::none()
            }
            Message::ToggleShortenPath(_) => {
                self.display_options.shorten_path ^= true;
                Task::none()
            }
            Message::ToggleDuplicates(_) => {
                self.filter_options.duplicates ^= true;
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
            Message::OpenFile(_, file) => {
                Task::perform(open_file(self.file_data_source.clone(), file), move |_| {
                    gui::Message::Noop
                })
            }
            Message::FilesLoaded(_, Ok(files)) => {
                self.files = files;
                self.is_offline = false;
                Task::none()
            }
            Message::FilesLoaded(_, Err(error)) => {
                tracing::error!("error while loading files from database: {error}");
                self.is_offline = true;
                Task::none()
            }
            Message::OrderBy(_, ordering) => {
                if self.display_options.order == ordering {
                    self.display_options.direction.toggle();
                } else {
                    self.display_options.order = ordering;
                    self.display_options.direction = Default::default();
                }
                Task::none()
            }
            Message::AddAllowTag(_, tag) => {
                self.filter_options.allow_tags.insert(tag);
                Task::none()
            }
            Message::RemoveAllowTag(_, tag) => {
                self.filter_options.allow_tags.retain(|t| t != &tag);
                Task::none()
            }
            Message::AddDenyTag(_, tag) => {
                self.filter_options.deny_tags.insert(tag);
                Task::none()
            }
            Message::RemoveDenyTag(_, tag) => {
                self.filter_options.deny_tags.retain(|t| t != &tag);
                Task::none()
            }
            Message::SetRegex(_, regex) => {
                let regex = regex.trim();
                if regex.is_empty() {
                    self.filter_options.regex = None;
                } else {
                    self.filter_options.regex = Some(regex.to_string());
                }
                Task::none()
            }
            Message::EditDialog(message) => match &mut self.dialog {
                Some(Dialog::EditFile(dialog)) => dialog.update(message),
                None => Task::none(),
            },
            Message::SetSelectionTag(_, tag) => {
                let tag = tag.trim();
                if tag.is_empty() {
                    self.selection_tag = None;
                } else {
                    self.selection_tag = Some(tag.to_string());
                }
                Task::none()
            }
            Message::AddTagToSelection(tab) => match self.selection_tag.take() {
                Some(tag) => Task::perform(
                    add_tag_to_selection(
                        self.file_data_source.clone(),
                        self.filtered_files().into_iter().cloned().collect(),
                        tag,
                    ),
                    move |result| match result {
                        Ok(()) => Message::LoadFiles(tab.clone()).into(),
                        Err(error) => Message::Error(tab.clone(), error).into(),
                    },
                ),
                None => Task::none(),
            },
            Message::DeleteTagFromSelection(tab) => match self.selection_tag.take() {
                Some(tag) => Task::perform(
                    delete_tag_from_selection(
                        self.file_data_source.clone(),
                        self.filtered_files().into_iter().cloned().collect(),
                        tag,
                    ),
                    move |result| match result {
                        Ok(()) => Message::LoadFiles(tab.clone()).into(),
                        Err(error) => Message::Error(tab.clone(), error).into(),
                    },
                ),
                None => Task::none(),
            },
            Message::FilterByReadingStatus(_, status, is_set) => {
                if is_set {
                    self.filter_options.reading_status.insert(status);
                } else {
                    self.filter_options.reading_status.remove(&status);
                }
                Task::none()
            }
        }
    }

    fn tag_pick_list<'a>(
        &'a self,
        message: impl Fn(String) -> gui::Message + 'a,
    ) -> PickList<'a, String, Vec<String>, String, gui::Message> {
        pick_list(
            self.all_tags()
                .into_iter()
                .filter(|tag| {
                    !self.filter_options.allow_tags.contains(tag)
                        && !self.filter_options.deny_tags.contains(tag)
                })
                .sorted()
                .collect::<Vec<_>>(),
            None::<String>,
            message,
        )
    }

    fn allowed_tags_filter_menu(&self) -> Element<gui::Message> {
        column![text("Allowed tags")]
            .apply_if(!self.filter_options.allow_tags.is_empty(), |col| {
                col.push(
                    self.filter_options
                        .allow_tags
                        .iter()
                        .fold(Column::new(), |col, tag| {
                            col.push(
                                button(text(tag).size(11))
                                    .padding(4)
                                    .style(delete_tag_button)
                                    .on_press(
                                        Message::RemoveAllowTag(self.tab(), tag.clone()).into(),
                                    ),
                            )
                        })
                        .spacing(5),
                )
            })
            .push(
                self.tag_pick_list(|tag| Message::AddAllowTag(self.tab(), tag).into())
                    .placeholder("Allow tag"),
            )
            .spacing(10)
            .into()
    }

    fn denied_tags_filter_menu(&self) -> Element<gui::Message> {
        column![text("Denied tags")]
            .apply_if(!self.filter_options.deny_tags.is_empty(), |col| {
                col.push(
                    self.filter_options
                        .deny_tags
                        .iter()
                        .fold(Column::new(), |col, tag| {
                            col.push(
                                button(text(tag).size(11))
                                    .padding(4)
                                    .style(add_tag_button)
                                    .on_press(
                                        Message::RemoveDenyTag(self.tab(), tag.clone()).into(),
                                    ),
                            )
                        })
                        .spacing(5),
                )
            })
            .push(
                self.tag_pick_list(|tag| Message::AddDenyTag(self.tab(), tag).into())
                    .placeholder("Deny tag"),
            )
            .spacing(10)
            .into()
    }

    pub fn view_menu(&self) -> Vec<Element<gui::Message>> {
        if self.is_offline {
            vec![
                container(
                    column![
                        text("Offline"),
                        button("Refresh")
                            .width(iced::Fill)
                            .on_press(Message::LoadFiles(self.tab()).into()),
                    ]
                    .spacing(5),
                )
                .style(container::rounded_box)
                .padding(10)
                .into(),
            ]
        } else {
            vec![
                container(
                    column![
                        text("Display Options"),
                        horizontal_rule(2),
                        checkbox("Hide Folder", self.display_options.shorten_path)
                            .width(iced::Fill)
                            .on_toggle(|_| Message::ToggleShortenPath(self.tab()).into()),
                    ]
                    .spacing(5),
                )
                .style(container::rounded_box)
                .padding(10)
                .into(),
                container(
                    column![
                        text("Filter Options"),
                        horizontal_rule(2),
                        checkbox("Duplicates", self.filter_options.duplicates)
                            .width(iced::Fill)
                            .on_toggle(|_| Message::ToggleDuplicates(self.tab()).into()),
                        horizontal_rule(2),
                        text_input(
                            "Regular expression",
                            self.filter_options
                                .regex
                                .as_ref()
                                .unwrap_or(&String::from("")),
                        )
                        .width(iced::Fill)
                        .on_input(|value| Message::SetRegex(self.tab(), value).into()),
                        horizontal_rule(2),
                        container(self.allowed_tags_filter_menu()),
                        horizontal_rule(2),
                        container(self.denied_tags_filter_menu()),
                        horizontal_rule(2),
                        container(ReadingStatus::iter().fold(
                            column![text("Reading Status")],
                            |column, status| {
                                column.push(
                                    checkbox(
                                        format!("{status}"),
                                        self.filter_options.reading_status.contains(&status),
                                    )
                                    .width(iced::Fill)
                                    .on_toggle(move |value| {
                                        Message::FilterByReadingStatus(self.tab(), status, value)
                                            .into()
                                    }),
                                )
                            }
                        ))
                    ]
                    .spacing(5),
                )
                .style(container::rounded_box)
                .padding(10)
                .into(),
                container(
                    column![
                        text("Tag Options"),
                        horizontal_rule(2),
                        text_input(
                            "Tag",
                            self.selection_tag.as_ref().unwrap_or(&String::from("")),
                        )
                        .width(iced::Fill)
                        .on_input(|value| Message::SetSelectionTag(self.tab(), value).into()),
                        row![
                            button("Delete")
                                .style(button::danger)
                                .width(iced::Fill)
                                .apply_if(self.selection_tag.is_some(), |this| this
                                    .on_press(Message::DeleteTagFromSelection(self.tab()).into())),
                            button("Add")
                                .style(button::success)
                                .width(iced::Fill)
                                .apply_if(self.selection_tag.is_some(), |this| this
                                    .on_press(Message::AddTagToSelection(self.tab()).into())),
                        ]
                        .spacing(5),
                    ]
                    .spacing(5),
                )
                .style(container::rounded_box)
                .padding(10)
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

        let mut grid = Column::new()
            .push(row![
                // button("id").on_press(Message::OrderBy(self.tab(), OrderFilesBy::Id).into()),
                // button("type").on_press(Message::OrderBy(self.tab(), OrderFilesBy::Type).into()),
                button("size").on_press(Message::OrderBy(self.tab(), OrderFilesBy::Size).into()),
                // button("fingerprint")
                //     .on_press(Message::OrderBy(self.tab(), OrderFilesBy::Fingerprint).into()),
                row![
                    button("Folder")
                        .on_press(Message::OrderBy(self.tab(), OrderFilesBy::Folder).into()),
                    button("Filename")
                        .on_press(Message::OrderBy(self.tab(), OrderFilesBy::Filename).into()),
                ]
                .spacing(5),
            ])
            .spacing(10);

        let files: Vec<_> = if self.filter_options.duplicates {
            let buckets: IndexMap<String, Vec<&File>> =
                to_buckets(self.filtered_files().into_iter(), |file| {
                    file.fingerprint.clone()
                });
            buckets
                .into_iter()
                .filter(|(_, values)| values.len() > 1)
                .flat_map(|(_, values)| values)
                .collect()
        } else {
            self.filtered_files()
        };

        for file in files {
            grid = grid.push(row![
                // text(file.id),
                // text(file.type_.clone()),
                text(file.size),
                // text(format!("{}...", &file.fingerprint[..9])),
                row![
                    button(display_path(
                        file.path.clone(),
                        self.display_options.shorten_path
                    ))
                    .style(button::text)
                    .on_press(
                        Message::OpenDialog(Dialog::edit_file(self.tab(), file.clone())).into()
                    )
                ]
                .extend(file.tags.iter().map(|tag| {
                    if self.filter_options.allow_tags.contains(tag) {
                        button(text(tag).size(11))
                            .padding(4)
                            .style(tag_button)
                            .into()
                    } else {
                        button(text(tag).size(11))
                            .padding(4)
                            .style(tag_button)
                            .on_press(Message::AddAllowTag(self.tab(), tag.clone()).into())
                            .into()
                    }
                }))
                .spacing(5),
            ]);
        }

        grid.into()
    }

    pub(crate) fn extend_breadcrumb<'a>(
        &'a self,
        breadcrumb: Row<'a, gui::Message>,
    ) -> Row<'a, gui::Message> {
        match &self.dialog {
            Some(Dialog::EditFile(dialog)) => dialog.extend_breadcrumb(breadcrumb),
            None => breadcrumb,
        }
    }
}

fn filter_files<'a>(
    files: &'a [File],
    display_options: &DisplayOptions,
    filter_options: &FilterOptions,
    settings: Arc<Settings>,
) -> Vec<&'a File> {
    let comp: fn(&File, &File) -> Ordering = match display_options.order {
        OrderFilesBy::Id => |f1, f2| f1.id.cmp(&f2.id),
        // OrderFilesBy::Type => |f1, f2| f1.type_.cmp(&f2.type_),
        OrderFilesBy::Filename => |f1, f2| {
            Path::new(&f1.path)
                .file_name()
                .map(OsStr::to_ascii_lowercase)
                .cmp(
                    &Path::new(&f2.path)
                        .file_name()
                        .map(OsStr::to_ascii_lowercase),
                )
        },
        OrderFilesBy::Folder => |f1, f2| f1.path.to_lowercase().cmp(&f2.path.to_lowercase()),
        OrderFilesBy::Size => |f1, f2| f1.size.cmp(&f2.size),
        // OrderFilesBy::Fingerprint => |f1, f2| f1.fingerprint.cmp(&f2.fingerprint),
    };

    let select_regex = filter_options
        .regex
        .as_ref()
        .and_then(|r| Regex::new(r).ok());

    files
        .iter()
        .filter(|file| {
            filter_options
                .allow_tags
                .iter()
                .all(|tag| file.tags.contains(tag))
        })
        .filter(|file| {
            !file
                .tags
                .iter()
                .any(|tag| filter_options.deny_tags.contains(tag))
        })
        .filter(|file| !settings.ui.contains_hidden_tag(&file.tags))
        .filter(|file| match &select_regex {
            Some(regex) => regex.is_match(&file.path),
            None => true,
        })
        .filter(|file| {
            filter_options.reading_status.is_empty()
                || filter_options.reading_status.contains(&file.status)
        })
        .sorted_by(|f1, f2| {
            comp(f1, f2).apply_if(
                display_options.direction == OrderDirection::Descending,
                Ordering::reverse,
            )
        })
        .collect()
}

async fn retrieve_files<FDS>(file_data_source: Arc<FDS>) -> Result<Vec<File>, Error>
where
    FDS: FileDataSource,
    <FDS as FileDataSource>::Error: 'static,
{
    let files = file_data_source
        .get_files()
        .await
        .map_err(|error| Error::DataSourceError(format!("{error}")))?;
    Ok(files)
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

async fn add_tag_to_selection<FDS>(
    file_data_source: Arc<FDS>,
    files: Vec<File>,
    tag: String,
) -> Result<(), Error>
where
    FDS: FileDataSource,
    <FDS as FileDataSource>::Error: 'static,
{
    for file in files {
        file_data_source
            .add_file_tags(file.id, vec![tag.clone()])
            .await
            .map_err(|error| Error::DataSourceError(format!("{error}")))?;
    }
    Ok(())
}

async fn delete_tag_from_selection<FDS>(
    file_data_source: Arc<FDS>,
    files: Vec<File>,
    tag: String,
) -> Result<(), Error>
where
    FDS: FileDataSource,
    <FDS as FileDataSource>::Error: 'static,
{
    for file in files {
        file_data_source
            .delete_file_tags(file.id, vec![tag.clone()])
            .await
            .map_err(|error| Error::DataSourceError(format!("{error}")))?;
    }
    Ok(())
}
