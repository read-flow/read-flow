use std::sync::Arc;

use iced::{
    widget::{button, column, radio, row, text, text_input, Column, Row},
    Element, Task,
};
use itertools::Itertools;
use strum::IntoEnumIterator;

use crate::{add_tag_button, delete_tag_button, tag_button, CurrentTabRef, IdentifyTab};
use archive_organizer::api::{FileDataSource, ReadingStatus};

use super::{display_path, CurrentTab, File};

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone)]
pub(crate) enum Message {
    EditTag(CurrentTab, String),
    AddTag(CurrentTab, Option<String>),
    DeleteTag(CurrentTab, String),
    Duplicates(CurrentTab, Vec<(CurrentTab, Vec<File>)>),
    Tags(CurrentTab, Vec<String>),
    SetStatus(CurrentTab, ReadingStatus),
}

impl IdentifyTab for Message {
    fn tab(&self) -> CurrentTab {
        match self {
            Message::EditTag(tab, ..) => tab.clone(),
            Message::AddTag(tab, ..) => tab.clone(),
            Message::DeleteTag(tab, ..) => tab.clone(),
            Message::Duplicates(tab, ..) => tab.clone(),
            Message::Tags(tab, ..) => tab.clone(),
            Message::SetStatus(tab, ..) => tab.clone(),
        }
    }
}

impl From<Message> for crate::Message {
    fn from(value: Message) -> Self {
        crate::Message::Files(super::Message::EditDialog(value))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct EditFile {
    tab: CurrentTab,
    file: File,
    tag: Option<String>,
    all_tags: Vec<String>,
    duplicates: Vec<(CurrentTab, Vec<File>)>,
}

impl EditFile {
    pub(crate) fn new(tab: CurrentTab, file: File) -> Self {
        Self {
            tab,
            file,
            tag: Default::default(),
            all_tags: Default::default(),
            duplicates: Default::default(),
        }
    }

    pub(crate) fn init(&self) -> Task<crate::Message> {
        Task::batch(vec![
            text_input::focus("input-tag"),
            Task::done(crate::Message::FindDuplicates(
                self.tab.clone(),
                self.file.fingerprint.clone(),
            )),
            Task::done(crate::Message::GetTags(self.tab.clone())),
        ])
    }

    pub(crate) fn view(&self) -> Element<crate::Message> {
        let tags_to_add = self
            .all_tags
            .iter()
            .filter(|tag| !self.file.tags.contains(tag))
            .sorted()
            .fold(Column::new(), |column, tag| {
                column.push(row![
                    button(text(tag).size(11)).padding(4).style(tag_button),
                    button(text(" + ").size(15))
                        .padding(1)
                        .style(add_tag_button)
                        .on_press(Message::AddTag(self.tab.clone(), tag.clone().into()).into())
                ])
            })
            .max_width(580.0)
            .spacing(10);

        let column = column![
            row![text("id"), text(self.file.id)],
            row![
                text("path"),
                button(display_path(self.file.path.clone(), false))
                    .style(button::text)
                    .on_press(super::Message::OpenFile(self.tab.clone(), self.file.clone()).into())
            ],
            row![text("type"), text(&self.file.type_)],
            row![text("size"), text(self.file.size)],
            row![
                text("status"),
                ReadingStatus::iter()
                    .fold(Column::new(), |column, status| column.push(radio(
                        format!("{status}"),
                        status,
                        Some(self.file.status),
                        |status| Message::SetStatus(self.tab.clone(), status).into(),
                    )))
                    .spacing(10)
            ],
            row![text("fingerprint"), text(&self.file.fingerprint)],
            row![
                text("tags"),
                column![self
                    .file
                    .tags
                    .iter()
                    .sorted()
                    .fold(Column::new(), |column, tag| {
                        column.push(row![
                            button(text(tag).size(11)).padding(4).style(tag_button),
                            button(text(" X ").size(15))
                                .padding(1)
                                .style(delete_tag_button)
                                .on_press(Message::DeleteTag(self.tab.clone(), tag.clone()).into())
                        ])
                    })
                    .max_width(580.0)
                    .spacing(10),]
                .spacing(10),
            ],
            row![
                text("create tag"),
                text_input("tag", &self.tag.clone().unwrap_or("".to_string()))
                    .width(250)
                    .id("input-tag")
                    .on_input(|result| Message::EditTag(self.tab.clone(), result).into())
                    .on_submit(Message::AddTag(self.tab.clone(), None).into()),
            ],
            row![text("existing tags"), tags_to_add],
            row![
                text("location"),
                CurrentTabRef::from(&self.tab).button_text()
            ],
            row![text("duplicates"), {
                self.duplicates
                    .iter()
                    .map(|(tab, duplicates)| {
                        (
                            tab,
                            duplicates
                                .iter()
                                .filter(|d| !(*tab == self.tab && d.path == self.file.path))
                                .collect::<Vec<_>>(),
                        )
                    })
                    .filter(|(_, duplicates)| !duplicates.is_empty())
                    .fold(column![], |col, (tab, duplicates)| {
                        let tab_ref: CurrentTabRef = tab.into();
                        col.push(row![
                            tab_ref.button_text(),
                            column![].extend(
                                duplicates
                                    .iter()
                                    .map(|d| { display_path(d.path.clone(), false) })
                            )
                        ])
                    })
                    .spacing(10)
            }],
            row![
                text(""),
                row![
                    button(text("Close"))
                        .style(button::secondary)
                        .on_press(super::Message::CancelDialog(self.tab().clone()).into()),
                    button(text("Save Changes"))
                        .style(button::primary)
                        .on_press(super::Message::SubmitDialog(self.tab().clone()).into()),
                ]
                .width(600.0)
                .spacing(10)
            ],
        ];

        column.spacing(10).into()
    }

    pub(super) fn update(&mut self, message: Message) -> Task<crate::Message> {
        match message {
            Message::EditTag(_, tag) => {
                self.tag = Some(tag);
                Task::none()
            }
            Message::AddTag(_, None) => {
                if let Some(tag) = self.tag.take() {
                    let tag = tag.trim();
                    if !tag.is_empty() {
                        self.file.tags.push(tag.to_owned());
                    }
                }
                Task::none()
            }
            Message::AddTag(_, Some(tag)) => {
                let tag = tag.trim();
                if !tag.is_empty() {
                    self.file.tags.push(tag.to_owned());
                }
                Task::none()
            }
            Message::DeleteTag(_, tag) => {
                self.file.tags.retain(|t| *t != tag);
                Task::none()
            }
            Message::Duplicates(_, duplicates) => {
                self.duplicates = duplicates;
                Task::none()
            }
            Message::Tags(_, tags) => {
                self.all_tags = tags;
                Task::none()
            }
            Message::SetStatus(_, status) => {
                self.file.status = status;
                Task::none()
            }
        }
    }

    pub(crate) fn submit<FDS>(self, file_data_source: Arc<FDS>) -> Task<crate::Message>
    where
        FDS: FileDataSource + Send + Sync + 'static,
        <FDS as FileDataSource>::Error: 'static,
    {
        Task::perform(
            super::update_file(file_data_source, self.file.clone()),
            move |result| match result {
                Ok(()) => super::Message::LoadFiles(self.tab().clone()).into(),
                Err(error) => super::Message::Error(self.tab().clone(), error).into(),
            },
        )
    }

    pub(crate) fn extend_breadcrumb<'a>(
        &self,
        breadcrumb: Row<'a, crate::Message>,
    ) -> Row<'a, crate::Message> {
        let breadcrumb = breadcrumb.push(text(" » "));
        breadcrumb.push(display_path(self.file.path.clone(), true))
    }
}

impl IdentifyTab for EditFile {
    fn tab(&self) -> CurrentTab {
        self.tab.clone()
    }
}
