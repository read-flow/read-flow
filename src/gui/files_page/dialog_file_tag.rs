use std::sync::Arc;

use iced::{
    widget::{button, column, container, row, text, text_input},
    Element, Task,
};

use crate::{
    api::{self, FileDataSource},
    gui::{self, CurrentTab, IdentifyTab},
};

use super::{add_file_tag, Message};

#[derive(Debug, Clone)]
pub(crate) struct FileTag {
    tab: CurrentTab,
    file_id: i32,
    tag: Option<String>,
}

impl FileTag {
    pub(crate) fn new(tab: CurrentTab, file_id: i32) -> Self {
        Self {
            tab,
            file_id,
            tag: None,
        }
    }

    pub(crate) fn update(&mut self, tag: String) -> Task<gui::Message> {
        self.tag = Some(tag);
        Task::none()
    }

    pub(crate) fn view(&self) -> Element<gui::Message> {
        let Self { tab, tag, .. } = self;
        container(
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
        .padding(10)
        .into()
    }

    pub(crate) fn close<FDS>(self, file_data_source: Arc<FDS>) -> Task<gui::Message>
    where
        FDS: FileDataSource + Send + Sync + 'static,
        <FDS as api::FileDataSource>::Error: 'static,
    {
        match self {
            FileTag {
                tab,
                tag: Some(tag),
                ..
            } if !tag.trim().is_empty() => Task::perform(
                add_file_tag(file_data_source, self.file_id, tag.trim().to_string()),
                move |result| Message::TagApplied(tab.clone(), result).into(),
            ),
            _ => Task::none(),
        }
    }
}

impl IdentifyTab for FileTag {
    fn tab(&self) -> CurrentTab {
        self.tab.clone()
    }
}
