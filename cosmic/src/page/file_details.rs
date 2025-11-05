use crate::client::ClientSelector;
use crate::fl;
use crate::page::get_file_type_icon;
use crate::state::LoadedState;
use crate::{app::ContextView, client::Client};
use archive_organizer::api::{File, FileDataSource, ReadingStatus};
use cosmic::{
    Action, Apply, Element, Task, cosmic_theme,
    iced::{
        Length,
        alignment::{Horizontal, Vertical},
        widget::combo_box,
    },
    iced_widget::{self, Column, Row},
    task, theme,
    widget::{self, text},
};
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
    RefreshFile(ClientSelector, File),
}

#[derive(Debug, Clone)]
pub enum FileDetailsMessage {
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
    OpenFile,

    // Message intended for the parent module
    Out(FileDetailsOutput),
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

        (file_details, task::message(FileDetailsMessage::LoadAllTags))
    }

    pub fn display_name(&self) -> String {
        Path::new(&self.file.path)
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("Unknown")
            .to_string()
    }

    pub fn view(&self) -> Element<'_, FileDetailsMessage> {
        let cosmic_theme::Spacing {
            space_xxs,
            space_xs,
            space_s,
            space_m,
            ..
        } = theme::active().cosmic().spacing;

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

        // Header with file icon, name, and actions
        let file_icon = get_file_type_icon(&self.file.type_);

        let header_card = widget::container(
            Column::new()
                .spacing(space_s)
                .push(
                    Row::new()
                        .spacing(space_s)
                        .align_y(Vertical::Center)
                        .push(widget::icon::from_name(file_icon).size(48).icon())
                        .push(
                            Column::new()
                                .spacing(space_xxs)
                                .push(
                                    text(filename_without_extension)
                                        .size(24)
                                        .width(Length::Fill),
                                )
                                .push(text(folder).size(14))
                                .width(Length::Fill),
                        )
                        .push(
                            Row::new()
                                .spacing(space_xs)
                                .push(
                                    widget::button::icon(widget::icon::from_name(
                                        "document-open-symbolic",
                                    ))
                                    .on_press(FileDetailsMessage::OpenFile)
                                    .tooltip(fl!("file-details-open-file")),
                                )
                                .push(
                                    widget::button::icon(widget::icon::from_name(
                                        "window-close-symbolic",
                                    ))
                                    .on_press(FileDetailsMessage::Out(FileDetailsOutput::Close(
                                        self.id,
                                    )))
                                    .tooltip(fl!("file-details-close")),
                                ),
                        ),
                )
                .push(
                    // Status indicator bar
                    Row::new()
                        .spacing(space_xs)
                        .align_y(Vertical::Center)
                        .push(text(format!("Status: {:?}", self.file.status)).size(14))
                        .push(text("•"))
                        .push(text(self.format_file_size(self.file.size.into())).size(14)),
                ),
        )
        .padding(space_m);

        // Main content with cards
        let main_content = Column::new()
            .spacing(space_s)
            .push(
                // File Information Card
                widget::container(
                    Column::new()
                        .spacing(space_s)
                        .push(
                            Row::new()
                                .spacing(space_xs)
                                .align_y(Vertical::Center)
                                .push(
                                    widget::icon::from_name("document-properties-symbolic")
                                        .size(20)
                                        .icon(),
                                )
                                .push(text(fl!("file-details-basic-info")).size(18)),
                        )
                        .push(
                            Column::new()
                                .spacing(space_xs)
                                .push(self.create_info_row(
                                    fl!("file-details-filename"),
                                    filename.to_string(),
                                ))
                                .push(self.create_info_row(
                                    fl!("file-details-type"),
                                    self.file.type_.clone(),
                                ))
                                .push(self.create_info_row(
                                    fl!("file-details-size"),
                                    self.format_file_size(self.file.size.into()),
                                ))
                                .push(
                                    // Reading status with visual picker
                                    Row::new()
                                        .spacing(space_s)
                                        .align_y(Vertical::Center)
                                        .push(
                                            text(fl!("file-details-status"))
                                                .width(Length::Fixed(120.0)),
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
                                            .width(Length::Fill)
                                            .placeholder(fl!("file-details-select-status")),
                                        ),
                                ),
                        ),
                )
                .padding(space_m),
            )
            .push(
                // Technical Details Card
                widget::container(
                    Column::new()
                        .spacing(space_s)
                        .push(
                            Row::new()
                                .spacing(space_xs)
                                .align_y(Vertical::Center)
                                .push(
                                    widget::icon::from_name("applications-engineering-symbolic")
                                        .size(20)
                                        .icon(),
                                )
                                .push(text(fl!("file-details-technical")).size(18)),
                        )
                        .push(
                            Column::new()
                                .spacing(space_xs)
                                .push(self.create_info_row(
                                    fl!("file-details-id"),
                                    format!("{}", self.file.id),
                                ))
                                .push(self.create_info_row(
                                    fl!("file-details-full-path"),
                                    self.file.path.clone(),
                                ))
                                .push(self.create_info_row(
                                    fl!("file-details-fingerprint"),
                                    self.file.fingerprint.clone(),
                                )),
                        ),
                )
                .padding(space_m),
            )
            .push(
                // Tags Card
                widget::container(
                    Column::new()
                        .spacing(space_s)
                        .push(
                            Row::new()
                                .spacing(space_xs)
                                .align_y(Vertical::Center)
                                .push(widget::icon::from_name("tag-symbolic").size(20).icon())
                                .push(text(fl!("file-details-tags")).size(18)),
                        )
                        .push(self.enhanced_tags_view()),
                )
                .padding(space_m),
            );

        // Main layout
        let content = Column::new()
            .spacing(space_s)
            .push(header_card)
            .push(main_content)
            .padding(space_m)
            .width(Length::Fill);

        // Wrap content in a scrollable container
        content
            .apply(widget::scrollable::vertical)
            .apply(widget::container)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Horizontal::Center)
            .align_y(Vertical::Top)
            .into()
    }

    pub fn view_context(&self) -> ContextView<'_, FileDetailsMessage> {
        ContextView {
            title: "FileDetails".to_string(),
            content: text("TODO").into(),
        }
    }

    pub fn update(&mut self, message: FileDetailsMessage) -> Task<Action<FileDetailsMessage>> {
        tracing::debug!("received: {message:?}");
        match message {
            FileDetailsMessage::Out(_) => {
                panic!("should be handled by the parent component")
            }
            FileDetailsMessage::UpdateReadingStatus(status) => {
                let mut updated_file = self.file.clone();
                updated_file.status = status;
                let client = self.client.clone();

                task::future(async move {
                    match client.update_file(updated_file).await {
                        Ok(()) => FileDetailsMessage::ReadingStatusUpdated(Ok(())),
                        Err(err) => FileDetailsMessage::ReadingStatusUpdated(Err(format!("{err}"))),
                    }
                })
            }
            FileDetailsMessage::ReadingStatusUpdated(result) => {
                match result {
                    Ok(()) => {
                        // Refresh the file to get updated status
                        task::message(FileDetailsMessage::RefreshFile)
                    }
                    Err(err) => {
                        tracing::error!("Failed to update reading status: {err}");
                        task::none()
                    }
                }
            }
            FileDetailsMessage::OpenFile => {
                let file = self.file.clone();
                let client = self.client.clone();
                task::future(async move {
                    if let Err(e) = client.xdg_open_file(file).await {
                        tracing::error!("Failed to open file: {e}");
                    }
                    FileDetailsMessage::RefreshFile
                })
            }
            FileDetailsMessage::LoadAllTags => {
                self.tags = TagsState::Loading;
                let client = self.client.clone();
                task::future(async move {
                    match client.get_files_tags().await {
                        Ok(tags) => FileDetailsMessage::AllTagsLoaded(Ok(tags)),
                        Err(err) => FileDetailsMessage::AllTagsLoaded(Err(format!("{err}"))),
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
                task::none()
            }
            FileDetailsMessage::UpdateNewTag(text) => {
                self.new_tag = text;
                task::none()
            }
            FileDetailsMessage::AddTag => {
                if self.new_tag.trim().is_empty() {
                    return task::none();
                }

                let id = self.file.id;
                let tag = self.new_tag.clone();
                let client = self.client.clone();

                self.new_tag = String::new();

                task::future(async move {
                    match client.add_file_tags(id, vec![tag]).await {
                        Ok(tags) => FileDetailsMessage::TagsAdded(Ok(tags)),
                        Err(err) => FileDetailsMessage::TagsAdded(Err(format!("{err}"))),
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
                        return task::message(FileDetailsMessage::RefreshFile);
                    }
                    Err(err) => {
                        tracing::warn!("Failed to add tag: {}", err);
                    }
                }
                task::none()
            }
            FileDetailsMessage::RemoveTag(tag) => {
                let id = self.file.id;
                let tag = tag.clone();
                let client = self.client.clone();

                task::future(async move {
                    match client.delete_file_tags(id, vec![tag]).await {
                        Ok(()) => FileDetailsMessage::TagsRemoved(Ok(())),
                        Err(err) => FileDetailsMessage::TagsRemoved(Err(format!("{err}"))),
                    }
                })
            }
            FileDetailsMessage::TagsRemoved(result) => {
                match result {
                    Ok(_) => {
                        // Refresh the file to get updated tags
                        return task::message(FileDetailsMessage::RefreshFile);
                    }
                    Err(err) => {
                        tracing::warn!("Failed to remove tag: {}", err);
                    }
                }
                task::none()
            }
            FileDetailsMessage::RefreshFile => {
                let id = self.file.id;
                let client = self.client.clone();

                task::future(async move {
                    match client.get_file(id).await {
                        Ok(file) => FileDetailsMessage::FileRefreshed(Ok(file)),
                        Err(err) => FileDetailsMessage::FileRefreshed(Err(format!("{err}"))),
                    }
                })
            }
            FileDetailsMessage::FileRefreshed(result) => match result {
                Ok(Some(file)) => {
                    self.file = file.clone();
                    task::message(FileDetailsMessage::Out(FileDetailsOutput::RefreshFile(
                        self.client.selector(),
                        file,
                    )))
                }
                Ok(None) => {
                    tracing::warn!("File not found during refresh");
                    Task::none()
                }
                Err(err) => {
                    tracing::warn!("Failed to refresh file: {}", err);
                    Task::none()
                }
            },
        }
    }
}

impl FileDetails {
    // Format file size in human-readable format
    fn format_file_size(&self, size: i64) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
        let mut size = size as f64;
        let mut unit_index = 0;

        while size >= 1024.0 && unit_index < UNITS.len() - 1 {
            size /= 1024.0;
            unit_index += 1;
        }

        if unit_index == 0 {
            format!("{} {}", size as i64, UNITS[unit_index])
        } else {
            format!("{:.1} {}", size, UNITS[unit_index])
        }
    }

    // Create a styled info row
    fn create_info_row(&self, label: String, value: String) -> Element<'_, FileDetailsMessage> {
        let cosmic_theme::Spacing { space_xs, .. } = theme::active().cosmic().spacing;

        Row::new()
            .spacing(space_xs)
            .align_y(Vertical::Center)
            .push(text(label).width(Length::Fixed(120.0)))
            .push(text(value).width(Length::Fill))
            .into()
    }

    // Enhanced tags view with better styling
    fn enhanced_tags_view(&self) -> Element<'_, FileDetailsMessage> {
        let cosmic_theme::Spacing {
            space_xs, space_s, ..
        } = theme::active().cosmic().spacing;
        let mut column = Column::new().spacing(space_s);

        // Show existing tags with enhanced styling
        if self.file.tags.is_empty() {
            column = column.push(
                widget::container(text(fl!("file-details-no-tags")))
                    .padding(space_s)
                    .width(Length::Fill),
            );
        } else {
            // Create a flow container for the tags with enhanced styling
            let mut tag_row = Row::new().spacing(space_xs).width(Length::Fill);
            for tag in &self.file.tags {
                let tag_button = widget::button::text(tag.clone())
                    .trailing_icon(widget::icon::from_name("edit-delete-symbolic"))
                    .on_press(FileDetailsMessage::RemoveTag(tag.clone()))
                    .tooltip(fl!("file-details-remove-tag"));

                tag_row = tag_row.push(tag_button);
            }
            column = column.push(tag_row);
        }

        // Add tag input section
        column =
            column.push(widget::container(iced_widget::horizontal_rule(1)).padding([space_s, 0]));

        column = match &self.tags {
            TagsState::Loaded(Tags { available_tags, .. }) => {
                // Add combo box for tag selection with enhanced styling
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

                let input_row = Row::new()
                    .push(combo)
                    .push(add_button)
                    .spacing(space_s)
                    .align_y(Vertical::Center);

                column.push(input_row)
            }
            TagsState::Loading => column.push(
                widget::container(
                    Row::new()
                        .spacing(space_xs)
                        .align_y(Vertical::Center)
                        .push(
                            widget::icon::from_name("content-loading-symbolic")
                                .size(16)
                                .icon(),
                        )
                        .push(text(fl!("file-details-loading-tags"))),
                )
                .padding(space_s),
            ),
            _ => column.push(widget::container(text("Failed to load tags")).padding(space_s)),
        };

        column.into()
    }
}
