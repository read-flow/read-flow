use std::{path::Path, sync::Arc};

use iced::{
    alignment::{Horizontal, Vertical},
    widget::{button, column, container, row, text, text_input},
    Element, Task,
};
use iced_aw::{grid, grid_row, Wrap};
use itertools::Itertools;

use crate::{
    api::FileDataSource,
    gui::{self, add_tag_button, delete_tag_button, tag_button, CurrentTabRef, IdentifyTab},
};

use super::{CurrentTab, File};

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone)]
pub(in crate::gui) enum Message {
    EditTag(CurrentTab, String),
    AddTag(CurrentTab, Option<String>),
    DeleteTag(CurrentTab, String),
    Duplicates(CurrentTab, Vec<(CurrentTab, Vec<File>)>),
    Tags(CurrentTab, Vec<String>),
}

impl IdentifyTab for Message {
    fn tab(&self) -> CurrentTab {
        match self {
            Message::EditTag(tab, ..) => tab.clone(),
            Message::AddTag(tab, ..) => tab.clone(),
            Message::DeleteTag(tab, ..) => tab.clone(),
            Message::Duplicates(tab, ..) => tab.clone(),
            Message::Tags(tab, ..) => tab.clone(),
        }
    }
}

impl From<Message> for gui::Message {
    fn from(value: Message) -> Self {
        gui::Message::Files(super::Message::EditDialog(value))
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

fn display_path(path: &str) -> Element<gui::Message> {
    let path = Path::new(path);
    let directory = path.parent().unwrap();
    let filename = path.file_name().unwrap();
    column![
        text(format!("{}", filename.to_string_lossy())),
        text(format!("{}", directory.display())).size(11),
    ]
    .spacing(5)
    .into()
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

    pub(crate) fn init(&self) -> Task<gui::Message> {
        Task::batch(vec![
            text_input::focus("input-tag"),
            Task::done(gui::Message::FindDuplicates(
                self.tab.clone(),
                self.file.fingerprint.clone(),
            )),
            Task::done(gui::Message::GetTags(self.tab.clone())),
        ])
    }

    pub(crate) fn view(&self) -> Element<gui::Message> {
        let wrap = self
            .all_tags
            .iter()
            .filter(|tag| !self.file.tags.contains(tag))
            .sorted()
            .fold(Wrap::new_vertical(), |wrap, tag| {
                wrap.push(container(row![
                    button(text(tag).size(11)).padding(4).style(tag_button),
                    button(text(" + ").size(15))
                        .padding(1)
                        .style(add_tag_button)
                        .on_press(Message::AddTag(self.tab.clone(), tag.clone().into()).into())
                ]))
            })
            .spacing(10)
            .line_spacing(10);

        let column = column![grid![
            grid_row![text("id"), text(self.file.id)],
            grid_row![text("path"), display_path(&self.file.path)],
            grid_row![text("type"), text(&self.file.type_)],
            grid_row![text("size"), text(self.file.size)],
            grid_row![text("fingerprint"), text(&self.file.fingerprint)],
            grid_row![
                text("tags"),
                container(
                    column![
                        row![]
                            .extend(self.file.tags.iter().map(|tag| {
                                container(row![
                                    button(text(tag).size(11)).padding(4).style(tag_button),
                                    button(text(" X ").size(15))
                                        .padding(1)
                                        .style(delete_tag_button)
                                        .on_press(
                                            Message::DeleteTag(self.tab.clone(), tag.clone())
                                                .into()
                                        )
                                ])
                                .into()
                            }))
                            .spacing(10),
                        text_input("tag", &self.tag.clone().unwrap_or("".to_string()))
                            .width(250)
                            .id("input-tag")
                            .on_input(|result| Message::EditTag(self.tab.clone(), result).into())
                            .on_submit(Message::AddTag(self.tab.clone(), None).into()),
                        row![
                            button(text("Cancel"))
                                .style(button::secondary)
                                .on_press(super::Message::CancelDialog(self.tab().clone()).into()),
                            button(text("Submit"))
                                .style(button::primary)
                                .on_press(super::Message::SubmitDialog(self.tab().clone()).into()),
                        ]
                        .spacing(10)
                    ]
                    .spacing(10)
                ),
            ],
            grid_row![text("existing tags"), container(wrap).height(100.0)],
            grid_row![
                text("location"),
                CurrentTabRef::from(&self.tab).button_text()
            ],
            grid_row![text("duplicates"), {
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
                    .fold(grid![], |grid, (tab, duplicates)| {
                        let tab_ref: CurrentTabRef = tab.into();
                        grid.push(grid_row![
                            tab_ref.button_text(),
                            column![].extend(duplicates.iter().map(|d| { display_path(&d.path) }))
                        ])
                    })
                    .horizontal_alignment(Horizontal::Left)
                    .vertical_alignment(Vertical::Top)
                    .spacing(10)
            }],
        ]
        .horizontal_alignment(Horizontal::Left)
        .vertical_alignment(Vertical::Top)
        .spacing(10)];

        column.spacing(10).into()
    }

    pub(super) fn update(&mut self, message: Message) -> Task<gui::Message> {
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
        }
    }

    pub(crate) fn submit<FDS>(self, file_data_source: Arc<FDS>) -> Task<gui::Message>
    where
        FDS: FileDataSource + Send + Sync + 'static,
        <FDS as FileDataSource>::Error: 'static,
    {
        Task::perform(
            super::update_file(file_data_source, self.file.clone()),
            move |result| match result {
                Ok(()) => super::Message::Update(self.tab().clone()).into(),
                Err(error) => super::Message::Error(self.tab().clone(), error).into(),
            },
        )
    }
}

impl IdentifyTab for EditFile {
    fn tab(&self) -> CurrentTab {
        self.tab.clone()
    }
}
