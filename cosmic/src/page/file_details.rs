use crate::client::Client;
use crate::fl;
use crate::state::LoadedState;
use archive_organizer::api::{File, FileDataSource, ReadingStatus};
use cosmic::{
    Action, Apply, Element, Task,
    iced::{
        Length,
        alignment::{Horizontal, Vertical},
        widget::combo_box,
    },
    iced_widget::{Column, Row},
    widget::{self, text},
};
use std::borrow::Cow;
use std::path::Path;

struct Tags {
    all_tags: Vec<String>,
    available_tags: combo_box::State<String>,
}

type TagsState = LoadedState<Tags>;

pub struct FileDetails {
    id: i32,
    file: File,
    client: Client,
    new_tag: String,
    tags: TagsState,
}

#[derive(Debug, Clone)]
pub enum FileDetailsOutput {
    Close(i32),
}

#[derive(Debug, Clone)]
pub enum FileDetailsMessage {
    Out(FileDetailsOutput),
    LoadAllTags,
    AllTagsLoaded(Result<Vec<String>, String>),
    UpdateNewTag(String),
    AddTag,
    TagsAdded(Result<Vec<String>, String>),
    RemoveTag(String),
    TagsRemoved(Result<(), String>),
    RefreshFile,
    FileRefreshed(Result<Option<File>, String>),
    UpdateReadingStatus(ReadingStatus),
    ReadingStatusUpdated(Result<(), String>),
}

impl FileDetails {
    pub fn new(id: i32, file: File, client: Client) -> (Self, Task<Action<FileDetailsMessage>>) {
        let file_details = FileDetails {
            id,
            file,
            client,
            new_tag: String::new(),
            tags: TagsState::default(),
        };

        (
            file_details,
            cosmic::task::message(FileDetailsMessage::LoadAllTags),
        )
    }

    pub fn display_name(&self) -> String {
        self.file.path.clone()
    }

    pub fn view(&self) -> Element<FileDetailsMessage> {
        // Extract filename and folder using std::path
        let path = Path::new(&self.file.path);

        // Get filename without extension
        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Unknown");

        let filename_without_extension = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or(filename);

        // Get the folder path
        let folder = path
            .parent()
            .and_then(|parent| parent.to_str())
            .unwrap_or("");

        // Prepare content
        let title_row = Row::with_children(vec![
            text(filename_without_extension)
                .size(24)
                .width(Length::Fill)
                .into(),
            cosmic::widget::button::icon(widget::icon::from_name("window-close-symbolic"))
                .on_press(FileDetailsMessage::Out(FileDetailsOutput::Close(self.id)))
                .into(),
        ]);

        let title = title_row.width(Length::Fill);

        let details = Column::with_children(vec![
            // Basic info section
            text(fl!("file-details-basic-info")).size(20).into(),
            cosmic::widget::container(Column::with_children(vec![
                row_with_label(fl!("file-details-folder"), folder),
                row_with_label(fl!("file-details-filename"), filename),
                row_with_label(fl!("file-details-type"), &self.file.type_),
                row_with_label(
                    fl!("file-details-size"),
                    fl!("file-details-size-bytes", size = self.file.size),
                ),
                // Reading status dropdown
                Row::new()
                    .push(
                        text(fl!("file-details-status"))
                            .width(Length::FillPortion(1))
                    )
                    .push(
                        cosmic::iced::widget::pick_list(
                            [
                                ReadingStatus::Unread,
                                ReadingStatus::Reading,
                                ReadingStatus::Read,
                            ],
                            Some(self.file.status),
                            FileDetailsMessage::UpdateReadingStatus,
                        )
                        .width(Length::FillPortion(2))
                        .placeholder(fl!("file-details-select-status")),
                    )
                    .spacing(10)
                    .padding([0, 0, 10, 0])
                    .into(),
            ]))
            .padding(10)
            .into(),
            // Technical details section
            text(fl!("file-details-technical")).size(20).into(),
            cosmic::widget::container(Column::with_children(vec![
                row_with_label(fl!("file-details-id"), format!("{}", self.file.id)),
                row_with_label(fl!("file-details-full-path"), &self.file.path),
                row_with_label(fl!("file-details-fingerprint"), &self.file.fingerprint),
            ]))
            .padding(10)
            .into(),
            // Tags section
            text(fl!("file-details-tags")).size(20).into(),
            cosmic::widget::container(self.tags_view())
                .padding(10)
                .into(),
        ]);

        let content = Column::with_children(vec![title.into(), details.into()])
            .spacing(20)
            .padding(20);

        // Wrap content in a container
        content
            .apply(cosmic::widget::scrollable)
            .apply(cosmic::widget::container)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Horizontal::Center)
            .align_y(Vertical::Top)
            .into()
    }

    pub fn update(&mut self, message: FileDetailsMessage) -> Task<Action<FileDetailsMessage>> {
        match message {
            FileDetailsMessage::Out(_) => {
                panic!("should be handled by the parent component")
            }
            FileDetailsMessage::UpdateReadingStatus(status) => {
                let mut updated_file = self.file.clone();
                updated_file.status = status;
                let client = self.client.clone();

                cosmic::task::future(async move {
                    match client.update_file(updated_file).await {
                        Ok(()) => FileDetailsMessage::ReadingStatusUpdated(Ok(())),
                        Err(err) => {
                            FileDetailsMessage::ReadingStatusUpdated(Err(format!("{}", err)))
                        }
                    }
                })
            }
            FileDetailsMessage::ReadingStatusUpdated(result) => {
                match result {
                    Ok(()) => {
                        // Refresh the file to get updated status
                        cosmic::task::message(FileDetailsMessage::RefreshFile)
                    }
                    Err(err) => {
                        tracing::error!("Failed to update reading status: {}", err);
                        cosmic::task::none()
                    }
                }
            }
            FileDetailsMessage::LoadAllTags => {
                self.tags = TagsState::Loading;
                let client = self.client.clone();
                cosmic::task::future(async move {
                    match client.get_files_tags().await {
                        Ok(tags) => FileDetailsMessage::AllTagsLoaded(Ok(tags)),
                        Err(err) => FileDetailsMessage::AllTagsLoaded(Err(format!("{}", err))),
                    }
                })
            }
            FileDetailsMessage::AllTagsLoaded(result) => {
                match result {
                    Ok(tags) => {
                        // Remove existing tags from options
                        let tags = tags
                            .iter()
                            .filter(|tag| !self.file.tags.contains(tag))
                            .cloned()
                            .collect::<Vec<_>>();
                        let available_tags = combo_box::State::new(tags.clone());
                        self.tags = TagsState::Loaded(Tags {
                            all_tags: tags,
                            available_tags,
                        });
                    }
                    Err(err) => {
                        tracing::warn!("Failed to load tags: {}", &err);
                        self.tags = TagsState::Failed(err);
                    }
                }
                cosmic::task::none()
            }
            FileDetailsMessage::UpdateNewTag(text) => {
                self.new_tag = text;
                cosmic::task::none()
            }
            FileDetailsMessage::AddTag => {
                if self.new_tag.trim().is_empty() {
                    return cosmic::task::none();
                }

                let id = self.file.id;
                let tag = self.new_tag.clone();
                let client = self.client.clone();

                self.new_tag = String::new();

                cosmic::task::future(async move {
                    match client.add_file_tags(id, vec![tag]).await {
                        Ok(tags) => FileDetailsMessage::TagsAdded(Ok(tags)),
                        Err(err) => FileDetailsMessage::TagsAdded(Err(format!("{}", err))),
                    }
                })
            }
            FileDetailsMessage::TagsAdded(result) => {
                match result {
                    Ok(tags) => {
                        if let TagsState::Loaded(Tags { all_tags, .. }) = &mut self.tags {
                            all_tags.extend(tags);
                            all_tags.dedup();
                        }
                        // Refresh the file to get updated tags
                        return cosmic::task::message(FileDetailsMessage::RefreshFile);
                    }
                    Err(err) => {
                        tracing::warn!("Failed to add tag: {}", err);
                    }
                }
                cosmic::task::none()
            }
            FileDetailsMessage::RemoveTag(tag) => {
                let id = self.file.id;
                let tag = tag.clone();
                let client = self.client.clone();

                cosmic::task::future(async move {
                    match client.delete_file_tags(id, vec![tag]).await {
                        Ok(()) => FileDetailsMessage::TagsRemoved(Ok(())),
                        Err(err) => FileDetailsMessage::TagsRemoved(Err(format!("{}", err))),
                    }
                })
            }
            FileDetailsMessage::TagsRemoved(result) => {
                match result {
                    Ok(_) => {
                        // Refresh the file to get updated tags
                        return cosmic::task::message(FileDetailsMessage::RefreshFile);
                    }
                    Err(err) => {
                        tracing::warn!("Failed to remove tag: {}", err);
                    }
                }
                cosmic::task::none()
            }
            FileDetailsMessage::RefreshFile => {
                let id = self.file.id;
                let client = self.client.clone();

                cosmic::task::future(async move {
                    match client.get_file(id).await {
                        Ok(file) => FileDetailsMessage::FileRefreshed(Ok(file)),
                        Err(err) => FileDetailsMessage::FileRefreshed(Err(format!("{}", err))),
                    }
                })
            }
            FileDetailsMessage::FileRefreshed(result) => {
                match result {
                    Ok(Some(file)) => {
                        self.file = file;
                    }
                    Ok(None) => {
                        tracing::warn!("File not found during refresh");
                    }
                    Err(err) => {
                        tracing::warn!("Failed to refresh file: {}", err);
                    }
                }
                Task::none()
            }
        }
    }
}

/// Helper function to create a row with a label and value
fn row_with_label<'a>(
    label: impl Into<Cow<'a, str>> + 'a,
    value: impl Into<Cow<'a, str>> + 'a,
) -> Element<'a, FileDetailsMessage> {
    Row::with_children(vec![
        text(label).width(Length::FillPortion(1)).into(),
        text(value).width(Length::FillPortion(3)).into(),
    ])
    .spacing(10)
    .padding(5)
    .into()
}

impl FileDetails {
    // Create a view for tags with add/remove functionality
    fn tags_view(&self) -> Element<FileDetailsMessage> {
        let mut column = Column::new().spacing(10);

        // Show existing tags with a remove button
        if self.file.tags.is_empty() {
            column = column.push(text(fl!("file-details-no-tags")));
        } else {
            // Create a flow container for the tags
            let mut tag_row = Row::new().spacing(5).width(Length::Fill);
            for tag in &self.file.tags {
                let tag_button = cosmic::widget::button::text(tag.clone())
                    .trailing_icon(widget::icon::from_name("edit-delete-symbolic"))
                    .on_press(FileDetailsMessage::RemoveTag(tag.clone()));

                tag_row = tag_row.push(tag_button);
            }
            column = column.push(tag_row);
        }

        // Add a divider
        column = column.push(cosmic::iced_widget::horizontal_rule(1).width(Length::Fill));

        column = match &self.tags {
            TagsState::Loaded(Tags { available_tags, .. }) => {
                // Add combo box for tag selection
                let combo = combo_box(
                    available_tags,
                    &fl!("file-details-select-tag"),
                    Some(&self.new_tag),
                    FileDetailsMessage::UpdateNewTag,
                )
                .width(Length::Fill);

                let add_button = widget::button::standard(fl!("file-details-add"))
                    .on_press(FileDetailsMessage::AddTag)
                    .width(Length::Shrink);

                let input_row = Row::new().push(combo).push(add_button).spacing(10);
                column.push(input_row)
            }
            _ => {
                // Show a loading indicator if needed
                column.push(text(fl!("file-details-loading-tags")))
            }
        };

        column.into()
    }
}
