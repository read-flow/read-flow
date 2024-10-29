mod dialog_file_tag;
mod page;

use std::sync::Arc;

use iced::Element;

use crate::{
    api::{File, FileDataSource},
    gui::{self, CurrentTab, IdentifyTab},
};

use super::tag_button;

use dialog_file_tag::FileTag;

pub use page::Page;

#[derive(Debug, Clone)]
pub(super) enum Message {
    Update(CurrentTab),
    ToggleShortenPath(CurrentTab),
    ToggleDuplicates(CurrentTab),
    CloseDialog(CurrentTab),
    OpenDialog(Dialog),
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
            Message::OpenDialog(dialog) => dialog.tab(),
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

#[derive(Debug, Clone, thiserror::Error)]
pub(super) enum Error {
    #[error("database error: {0}")]
    DataSourceError(String),
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

#[derive(Debug, Clone)]
pub(super) enum Dialog {
    FileTag(FileTag),
}

impl IdentifyTab for Dialog {
    fn tab(&self) -> CurrentTab {
        match self {
            Self::FileTag(dialog) => dialog.tab(),
        }
    }
}

impl Dialog {
    fn file_tag(tab: CurrentTab, file_id: i32) -> Self {
        Self::FileTag(FileTag::new(tab, file_id))
    }

    fn view(&self) -> Element<gui::Message> {
        match self {
            Dialog::FileTag(file_tag) => file_tag.view(),
        }
    }
}
