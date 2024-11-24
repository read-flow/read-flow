pub(super) mod dialog_edit_file;
mod page;

use std::{path::Path, sync::Arc};

use iced::{
    widget::{column, text},
    Element, Task,
};

use crate::{
    api::{File, FileDataSource},
    gui::{self, CurrentTab, IdentifyTab},
};

use super::tag_button;

use dialog_edit_file::EditFile;

pub use page::Page;

#[derive(Debug, Clone)]
pub(super) enum Message {
    Update(CurrentTab),
    Error(CurrentTab, Error),
    ToggleShortenPath(CurrentTab),
    ToggleDuplicates(CurrentTab),
    CancelDialog(CurrentTab),
    SubmitDialog(CurrentTab),
    OpenDialog(Dialog),
    OpenFile(CurrentTab, File),
    FilesLoaded(CurrentTab, Result<Vec<File>, Error>),
    OrderBy(CurrentTab, OrderFilesBy),
    AddTagFilter(CurrentTab, String),
    RemoveTagFilter(CurrentTab, String),
    SetRegex(CurrentTab, String),
    EditDialog(dialog_edit_file::Message),
    SetSelectionTag(CurrentTab, String),
    AddTagToSelection(CurrentTab),
    DeleteTagFromSelection(CurrentTab),
}

impl IdentifyTab for Message {
    fn tab(&self) -> CurrentTab {
        match self {
            Message::Update(tab) => tab.clone(),
            Message::Error(tab, ..) => tab.clone(),
            Message::ToggleShortenPath(tab) => tab.clone(),
            Message::ToggleDuplicates(tab) => tab.clone(),
            Message::CancelDialog(tab) => tab.clone(),
            Message::SubmitDialog(tab) => tab.clone(),
            Message::OpenDialog(dialog) => dialog.tab(),
            Message::OpenFile(tab, ..) => tab.clone(),
            Message::FilesLoaded(tab, ..) => tab.clone(),
            Message::OrderBy(tab, ..) => tab.clone(),
            Message::AddTagFilter(tab, ..) => tab.clone(),
            Message::RemoveTagFilter(tab, ..) => tab.clone(),
            Message::SetRegex(tab, ..) => tab.clone(),
            Message::EditDialog(message) => message.tab(),
            Message::SetSelectionTag(tab, ..) => tab.clone(),
            Message::AddTagToSelection(tab) => tab.clone(),
            Message::DeleteTagFromSelection(tab) => tab.clone(),
        }
    }
}

impl From<Message> for gui::Message {
    fn from(value: Message) -> Self {
        gui::Message::Files(value)
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
    Type,
    Filename,
    Folder,
    Size,
    Fingerprint,
}

fn display_path<P: AsRef<Path>>(path: P) -> Element<'static, gui::Message> {
    let path = path.as_ref();
    let directory = path.parent().unwrap();
    let filename = path.file_name().unwrap();
    if directory.is_dir() {
        column![
            text(format!("{}", filename.to_string_lossy())),
            text(format!("{}", directory.display())).size(11),
        ]
        .spacing(5)
        .into()
    } else {
        text(format!("{}", filename.to_string_lossy())).into()
    }
}

async fn update_file<FDS>(file_data_source: Arc<FDS>, file: File) -> Result<(), Error>
where
    FDS: FileDataSource,
    <FDS as FileDataSource>::Error: 'static,
{
    match file_data_source
        .get_file(file.id)
        .await
        .map_err(|error| Error::DataSourceError(format!("{error}")))?
    {
        None => todo!(),
        Some(original_file) => {
            let mut tags_to_delete = original_file.tags.clone();
            tags_to_delete.retain(|t| !file.tags.contains(t));
            tracing::warn!("tags to delete: {tags_to_delete:?}");

            file_data_source
                .delete_file_tags(file.id, tags_to_delete)
                .await
                .map_err(|error| Error::DataSourceError(format!("{error}")))?;

            let mut tags_to_insert = file.tags.clone();
            tags_to_insert.retain(|t| !original_file.tags.contains(t));
            tracing::warn!("tags to insert: {tags_to_insert:?}");

            file_data_source
                .add_file_tags(file.id, tags_to_insert)
                .await
                .map_err(|error| Error::DataSourceError(format!("{error}")))?;
        }
    }
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

    fn init(&self) -> Task<gui::Message> {
        match self {
            Dialog::EditFile(dialog) => dialog.init(),
        }
    }

    fn view(&self) -> Element<gui::Message> {
        match self {
            Dialog::EditFile(dialog) => dialog.view(),
        }
    }
}
