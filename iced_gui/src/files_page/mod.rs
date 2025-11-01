pub(super) mod dialog_edit_file;
mod page;

use std::{path::Path, sync::Arc};

use iced::{
    Element, Task,
    widget::{column, text},
};

use crate::{CurrentTab, IdentifyTab, Message as GuiMessage};
use archive_organizer::api::{File, FileDataSource, ReadingStatus};

use super::tag_button;

use dialog_edit_file::EditFile;

pub use page::Page;

#[derive(Debug, Clone)]
pub(super) enum Message {
    LoadFiles(CurrentTab),
    Error(CurrentTab, Error),
    ToggleShortenPath(CurrentTab),
    ToggleDuplicates(CurrentTab),
    CancelDialog(CurrentTab),
    SubmitDialog(CurrentTab),
    OpenDialog(Dialog),
    OpenFile(CurrentTab, File),
    FilesLoaded(CurrentTab, Result<Vec<File>, Error>),
    OrderBy(CurrentTab, OrderFilesBy),
    AddAllowTag(CurrentTab, String),
    RemoveAllowTag(CurrentTab, String),
    AddDenyTag(CurrentTab, String),
    RemoveDenyTag(CurrentTab, String),
    SetRegex(CurrentTab, String),
    EditDialog(dialog_edit_file::Message),
    SetSelectionTag(CurrentTab, String),
    AddTagToSelection(CurrentTab),
    DeleteTagFromSelection(CurrentTab),
    FilterByReadingStatus(CurrentTab, ReadingStatus, bool),
}

impl IdentifyTab for Message {
    fn tab(&self) -> CurrentTab {
        match self {
            Message::LoadFiles(tab) => tab.clone(),
            Message::Error(tab, ..) => tab.clone(),
            Message::ToggleShortenPath(tab) => tab.clone(),
            Message::ToggleDuplicates(tab) => tab.clone(),
            Message::CancelDialog(tab) => tab.clone(),
            Message::SubmitDialog(tab) => tab.clone(),
            Message::OpenDialog(dialog) => dialog.tab(),
            Message::OpenFile(tab, ..) => tab.clone(),
            Message::FilesLoaded(tab, ..) => tab.clone(),
            Message::OrderBy(tab, ..) => tab.clone(),
            Message::AddAllowTag(tab, ..) => tab.clone(),
            Message::RemoveAllowTag(tab, ..) => tab.clone(),
            Message::AddDenyTag(tab, ..) => tab.clone(),
            Message::RemoveDenyTag(tab, ..) => tab.clone(),
            Message::SetRegex(tab, ..) => tab.clone(),
            Message::EditDialog(message) => message.tab(),
            Message::SetSelectionTag(tab, ..) => tab.clone(),
            Message::AddTagToSelection(tab) => tab.clone(),
            Message::DeleteTagFromSelection(tab) => tab.clone(),
            Message::FilterByReadingStatus(tab, ..) => tab.clone(),
        }
    }
}

impl From<Message> for GuiMessage {
    fn from(value: Message) -> Self {
        GuiMessage::Files(value)
    }
}

impl From<(CurrentTab, Vec<(CurrentTab, Vec<File>)>)> for Message {
    fn from((tab, duplicates): (CurrentTab, Vec<(CurrentTab, Vec<File>)>)) -> Self {
        Message::EditDialog(dialog_edit_file::Message::Duplicates(tab, duplicates))
    }
}

impl From<(CurrentTab, Vec<String>)> for Message {
    fn from((tab, tags): (CurrentTab, Vec<String>)) -> Self {
        Message::EditDialog(dialog_edit_file::Message::Tags(tab, tags))
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub(super) enum Error {
    #[error("database error: {0}")]
    DataSourceError(String),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum OrderDirection {
    #[default]
    Ascending,
    Descending,
}

impl OrderDirection {
    fn toggle(&mut self) {
        match self {
            OrderDirection::Ascending => *self = OrderDirection::Descending,
            OrderDirection::Descending => *self = OrderDirection::Ascending,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum OrderFilesBy {
    #[default]
    Id,
    // Type,
    Filename,
    Folder,
    Size,
    // Fingerprint,
}

fn display_path<P: AsRef<Path>>(path: P, shorten_path: bool) -> Element<'static, GuiMessage> {
    let path = path.as_ref();
    let directory = format!("{}", path.parent().unwrap().display());
    let filename = path.file_name().unwrap();
    if shorten_path {
        text(format!("{}", filename.to_string_lossy())).into()
    } else {
        column![
            text(format!("{}", filename.to_string_lossy())),
            text(directory).size(11),
        ]
        .spacing(5)
        .into()
    }
}

async fn update_file<FDS>(file_data_source: Arc<FDS>, file: File) -> Result<(), Error>
where
    FDS: FileDataSource,
    <FDS as FileDataSource>::Error: 'static,
{
    file_data_source
        .update_file(file.clone())
        .await
        .map_err(|error| Error::DataSourceError(format!("{error}")))?;
    Ok(())
}

#[derive(Debug, Clone)]
pub(super) enum Dialog {
    EditFile(EditFile),
}

impl IdentifyTab for Dialog {
    fn tab(&self) -> CurrentTab {
        match self {
            Self::EditFile(dialog) => dialog.tab(),
        }
    }
}

impl Dialog {
    fn edit_file(tab: CurrentTab, file: File) -> Self {
        Self::EditFile(EditFile::new(tab, file))
    }

    fn init(&self) -> Task<GuiMessage> {
        match self {
            Dialog::EditFile(dialog) => dialog.init(),
        }
    }

    fn view(&self) -> Element<'_, GuiMessage> {
        match self {
            Dialog::EditFile(dialog) => dialog.view(),
        }
    }
}
