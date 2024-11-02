use std::sync::Arc;

use iced::{
    widget::{button, column, container, row, text, text_input},
    Element, Task,
};
use iced_aw::{grid, grid_row};

use crate::{
    api::FileDataSource,
    gui::{self, delete_tag_button, tag_button, IdentifyTab},
};

use super::{CurrentTab, File, Message};

#[derive(Debug, Clone)]
pub(crate) struct EditFile {
    tab: CurrentTab,
    file: File,
    tag: Option<String>,
}

impl EditFile {
    pub(crate) fn new(tab: CurrentTab, file: File) -> Self {
        Self {
            tab,
            file,
            tag: None,
        }
    }

    pub(crate) fn init(&self) -> Task<gui::Message> {
        text_input::focus("input-tag")
    }

    pub(crate) fn view(&self) -> Element<gui::Message> {
        container(
            grid![
                grid_row![text("id"), text(self.file.id)],
                grid_row![text("path"), text(&self.file.path)],
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
                                    button(text("X").size(11)).padding(4).style(delete_tag_button).on_press(
                                        Message::DeleteTag(self.tab.clone(), tag.clone()).into()
                                    )
                                ])
                                .into()
                            }))
                            .spacing(10),
                        text_input("tag", &self.tag.clone().unwrap_or("".to_string()))
                            .width(250)
                            .id("input-tag")
                            .on_input(|result| Message::EditTag(self.tab.clone(), result).into())
                            .on_submit(Message::AddTag(self.tab.clone()).into())
                    ]
                        .spacing(10)
                    ),
                ],
                grid_row![
                    text(""),
                    row![
                        button(text("Cancel"))
                            .style(button::secondary)
                            .on_press(Message::CancelDialog(self.tab().clone()).into()),
                        button(text("Submit"))
                            .style(button::primary)
                            .on_press(Message::SubmitDialog(self.tab().clone()).into()),
                    ]
                    .spacing(10)
                ]
            ]
            .spacing(10),
        )
        .into()
    }

    pub(crate) fn edit_tag(&mut self, tag: String) -> Task<gui::Message> {
        self.tag = Some(tag);
        Task::none()
    }

    pub(crate) fn submit<FDS>(self, file_data_source: Arc<FDS>) -> Task<gui::Message>
    where
        FDS: FileDataSource + Send + Sync + 'static,
        <FDS as FileDataSource>::Error: 'static,
    {
        Task::perform(
            super::update_file(file_data_source, self.file.clone()),
            move |result| match result {
                Ok(()) => Message::Update(self.tab().clone()).into(),
                Err(error) => Message::Error(self.tab().clone(), error).into(),
            },
        )
    }

    pub(crate) fn add_tag(&mut self) -> Task<gui::Message> {
        if let Some(tag) = self.tag.take() {
            let tag = tag.trim();
            if !tag.is_empty() {
                self.file.tags.push(tag.to_owned());
            }
        }
        Task::none()
    }

    pub(crate) fn delete_tag(&mut self, tag: String) -> Task<gui::Message> {
        self.file.tags.retain(|t| *t != tag);
        Task::none()
    }
}

impl IdentifyTab for EditFile {
    fn tab(&self) -> CurrentTab {
        self.tab.clone()
    }
}
