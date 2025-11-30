use std::path::Path;

use archive_organizer::Builder;
use archive_organizer::api::ReadingStatus;
use cosmic::Action;
use cosmic::Apply;
use cosmic::Element;
use cosmic::Task;
use cosmic::cosmic_theme;
use cosmic::iced::Length;
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::alignment::Vertical;
use cosmic::iced::widget::combo_box;
use cosmic::iced_widget;
use cosmic::iced_widget::Column;
use cosmic::iced_widget::Row;
use cosmic::task;
use cosmic::theme;
use cosmic::widget;
use cosmic::widget::text;

use crate::aggregator::Aggregator;
use crate::aggregator::Document;
use crate::app::ContextView;
use crate::fl;
use crate::state::LoadedState;

struct Tags {
    all_tags: Vec<String>,
    available_tags: combo_box::State<String>,
}

type TagsState = LoadedState<Tags>;

pub struct DocumentDetails {
    document: Document,
    aggregator: Aggregator,
    selected_tag: String,
    entered_tag: String,
    tags: TagsState,
}

#[derive(Debug, Clone)]
pub enum DocumentDetailsOutput {
    Close(String), // Fingerprint
    RefreshDocument(Document),
}

#[derive(Debug, Clone)]
pub enum DocumentDetailsMessage {
    LoadAllTags,
    AllTagsLoaded(Result<Vec<String>, String>),
    UpdateSelectedTag(String),
    AddSelectedTag,
    UpdateEnteredTag(String),
    AddEnteredTag(String),
    TagsAdded(Result<Vec<String>, String>),
    RemoveTag(String),
    TagsRemoved(Result<(), String>),
    RefreshDocument,
    DocumentRefreshed(Result<Document, String>),
    UpdateReadingStatus(ReadingStatus),
    ReadingStatusUpdated(Result<(), String>),
    OpenDocument,

    // Message intended for the parent module
    Out(DocumentDetailsOutput),
}

impl DocumentDetails {
    pub fn new(
        document: Document,
        aggregator: Aggregator,
    ) -> (Self, Task<Action<DocumentDetailsMessage>>) {
        let file_details = DocumentDetails {
            document,
            aggregator,
            selected_tag: String::new(),
            entered_tag: String::new(),
            tags: TagsState::default(),
        };

        (
            file_details,
            task::message(DocumentDetailsMessage::LoadAllTags),
        )
    }

    pub fn display_name(&self) -> String {
        Path::new(&self.document.sources.iter().next().unwrap().path)
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("Unknown")
            .to_string()
    }

    pub fn view(&self) -> Element<'_, DocumentDetailsMessage> {
        let cosmic_theme::Spacing {
            space_xxs,
            space_xs,
            space_s,
            ..
        } = theme::active().cosmic().spacing;

        // Extract filename and folder using std::path
        let path = Path::new(&self.document.sources.iter().next().unwrap().path);

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
        let file_icon = self.document.metadata.type_.get_file_type_icon();

        let header = Row::new()
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
                        widget::button::icon(widget::icon::from_name("document-open-symbolic"))
                            .on_press(DocumentDetailsMessage::OpenDocument)
                            .tooltip(fl!("file-details-open-file")),
                    )
                    .push(
                        widget::button::icon(widget::icon::from_name("window-close-symbolic"))
                            .on_press(DocumentDetailsMessage::Out(DocumentDetailsOutput::Close(
                                self.document.metadata.fingerprint.clone(),
                            )))
                            .tooltip(fl!("file-details-close")),
                    ),
            );

        // Build settings sections
        let basic_info_section = widget::settings::section()
            .title(fl!("file-details-basic-info"))
            .add(widget::settings::item(
                fl!("file-details-filename"),
                text(filename),
            ))
            .add(widget::settings::item(
                fl!("file-details-folder"),
                text(folder),
            ))
            .add(widget::settings::item(
                fl!("file-details-type"),
                text(self.document.metadata.type_.as_str()),
            ))
            .add(widget::settings::item(
                fl!("file-details-size"),
                text(self.format_file_size(self.document.metadata.size.into())),
            ))
            .add(widget::settings::item(
                fl!("file-details-status"),
                cosmic::iced::widget::pick_list(
                    [
                        ReadingStatus::Unread,
                        ReadingStatus::Reading,
                        ReadingStatus::Read,
                    ],
                    Some(self.document.metadata.status),
                    DocumentDetailsMessage::UpdateReadingStatus,
                )
                .placeholder(fl!("file-details-select-status")),
            ));

        let technical_section = widget::settings::section()
            .title(fl!("file-details-technical"))
            .add(widget::settings::item(
                fl!("file-details-fingerprint"),
                text(&self.document.metadata.fingerprint),
            ));

        let tags_section = widget::settings::section()
            .title(fl!("file-details-tags"))
            .add(self.tags_view());

        // Main layout using settings view_column
        let content = widget::settings::view_column(vec![
            header.into(),
            basic_info_section.into(),
            technical_section.into(),
            tags_section.into(),
        ]);

        // Wrap content in a scrollable container
        vec![
            widget::horizontal_space().into(),
            content
                .apply(widget::scrollable::vertical)
                .apply(widget::container)
                .width(Length::FillPortion(4))
                .height(Length::Fill)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Top)
                .into(),
            widget::horizontal_space().into(),
        ]
        .apply(Row::with_children)
        .into()
    }

    pub fn view_context(&self) -> ContextView<'_, DocumentDetailsMessage> {
        ContextView {
            title: "FileDetails".to_string(),
            content: text("TODO").into(),
        }
    }

    pub fn update(
        &mut self,
        message: DocumentDetailsMessage,
    ) -> Task<Action<DocumentDetailsMessage>> {
        tracing::debug!("received: {message:?}");
        match message {
            DocumentDetailsMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent component")
            }
            DocumentDetailsMessage::UpdateReadingStatus(status) => {
                let mut updated_document = self.document.clone();
                updated_document.metadata.status = status;
                let aggregator = self.aggregator.clone();

                task::future(async move {
                    let result = aggregator
                        .update_document(updated_document)
                        .await
                        .map_err(|err| format!("{err}"));
                    DocumentDetailsMessage::ReadingStatusUpdated(result)
                })
            }
            DocumentDetailsMessage::ReadingStatusUpdated(result) => {
                match result {
                    Ok(()) => {
                        // Refresh the file to get updated status
                        task::message(DocumentDetailsMessage::RefreshDocument)
                    }
                    Err(err) => {
                        tracing::error!("Failed to update reading status: {err}");
                        task::none()
                    }
                }
            }
            DocumentDetailsMessage::OpenDocument => {
                let document = self.document.clone();
                let aggregator = self.aggregator.clone();
                task::future(async move {
                    if let Err(e) = aggregator.xdg_open_file(document).await {
                        tracing::error!("Failed to open file: {e}");
                    }
                    DocumentDetailsMessage::RefreshDocument
                })
            }
            DocumentDetailsMessage::LoadAllTags => {
                self.tags = TagsState::Loading;
                let aggregator = self.aggregator.clone();
                task::future(async move {
                    let result = aggregator
                        .get_file_tags()
                        .await
                        .map_err(|err| format!("{err}"));
                    DocumentDetailsMessage::AllTagsLoaded(result)
                })
            }
            DocumentDetailsMessage::AllTagsLoaded(result) => {
                match result {
                    Ok(tags) => {
                        // Remove existing tags from options
                        let tags = tags
                            .iter()
                            .filter(|tag| !self.document.metadata.tags.contains(tag))
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
            DocumentDetailsMessage::UpdateSelectedTag(text) => {
                self.selected_tag = text;
                task::none()
            }
            DocumentDetailsMessage::AddSelectedTag => {
                if self.selected_tag.trim().is_empty() {
                    return task::none();
                }

                let tag = self.selected_tag.clone();
                self.selected_tag = String::new();

                self.add_tag(tag)
            }
            DocumentDetailsMessage::TagsAdded(result) => {
                match result {
                    Ok(tags) => {
                        if let TagsState::Loaded(Tags { all_tags, .. }) = &mut self.tags {
                            all_tags.extend(tags);
                            all_tags.dedup();
                        }
                        // Refresh the file to get updated tags
                        return task::message(DocumentDetailsMessage::RefreshDocument);
                    }
                    Err(err) => {
                        tracing::warn!("Failed to add tag: {}", err);
                    }
                }
                task::none()
            }
            DocumentDetailsMessage::RemoveTag(tag) => {
                let tag = tag.clone();
                let document = self.document.clone();
                let aggregator = self.aggregator.clone();

                task::future(async move {
                    // TODO: extract map_err and creation of message for result into extension function
                    let result = aggregator
                        .delete_document_tags(document, vec![tag])
                        .await
                        .map_err(|err| format!("{err}"));
                    DocumentDetailsMessage::TagsRemoved(result)
                })
            }
            DocumentDetailsMessage::TagsRemoved(result) => {
                match result {
                    Ok(_) => {
                        // Refresh the file to get updated tags
                        return task::message(DocumentDetailsMessage::RefreshDocument);
                    }
                    Err(err) => {
                        tracing::warn!("Failed to remove tag: {}", err);
                    }
                }
                task::none()
            }
            DocumentDetailsMessage::RefreshDocument => {
                let document = self.document.clone();
                let aggregator = self.aggregator.clone();

                task::future(async move {
                    let result = aggregator
                        .reload_document(document)
                        .await
                        .map_err(|err| format!("{err}"));
                    DocumentDetailsMessage::DocumentRefreshed(result)
                })
            }
            DocumentDetailsMessage::DocumentRefreshed(result) => match result {
                Ok(document) => {
                    self.document = document.clone();
                    task::message(DocumentDetailsMessage::Out(
                        DocumentDetailsOutput::RefreshDocument(document),
                    ))
                }
                Err(err) => {
                    tracing::warn!("Failed to refresh file: {}", err);
                    Task::none()
                }
            },
            DocumentDetailsMessage::UpdateEnteredTag(tag) => {
                self.entered_tag = tag;
                Task::none()
            }
            DocumentDetailsMessage::AddEnteredTag(tag) => {
                self.entered_tag.clear();
                self.add_tag(tag)
            }
        }
    }

    fn add_tag(&mut self, tag: String) -> Task<Action<DocumentDetailsMessage>> {
        let document = self.document.clone();
        let client = self.aggregator.clone();

        task::future(async move {
            match client.add_document_tags(document, vec![tag]).await {
                Ok(tags) => DocumentDetailsMessage::TagsAdded(Ok(tags)),
                Err(err) => DocumentDetailsMessage::TagsAdded(Err(format!("{err}"))),
            }
        })
    }
}

impl DocumentDetails {
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

    // Tags view for settings section
    fn tags_view(&self) -> Element<'_, DocumentDetailsMessage> {
        let cosmic_theme::Spacing {
            space_xs, space_s, ..
        } = theme::active().cosmic().spacing;
        let mut column = Column::new().spacing(space_s);

        // Show existing tags
        if self.document.metadata.tags.is_empty() {
            column = column.push(text(fl!("file-details-no-tags")));
        } else {
            // Create a flow container for the tags
            let mut tag_row = Row::new().spacing(space_xs).width(Length::Fill);
            for tag in &self.document.metadata.tags {
                let tag_button = widget::button::text(tag.clone())
                    .trailing_icon(widget::icon::from_name("edit-delete-symbolic"))
                    .on_press(DocumentDetailsMessage::RemoveTag(tag.clone()))
                    .tooltip(fl!("file-details-remove-tag"));

                tag_row = tag_row.push(tag_button);
            }
            column = column.push(tag_row);
        }

        // Add tag input section
        column = column.push(iced_widget::horizontal_rule(1));

        column = match &self.tags {
            TagsState::Loaded(Tags { available_tags, .. }) => {
                // Add combo box for tag selection
                let combo = combo_box(
                    available_tags,
                    &fl!("file-details-select-tag"),
                    Some(&self.selected_tag),
                    DocumentDetailsMessage::UpdateSelectedTag,
                )
                .width(Length::Fill);

                let add_button = widget::button::standard(fl!("file-details-add"))
                    .apply_if(!self.selected_tag.is_empty(), |button| {
                        button
                            .on_press(DocumentDetailsMessage::AddSelectedTag)
                            .class(widget::button::ButtonClass::Suggested)
                    })
                    .width(Length::Shrink);

                let input_row = Row::new()
                    .push(combo)
                    .push(add_button)
                    .spacing(space_s)
                    .align_y(Vertical::Center);

                column = column.push(input_row);

                let input = widget::text_input(fl!("file-details-enter"), &self.entered_tag)
                    .on_input(DocumentDetailsMessage::UpdateEnteredTag)
                    .on_submit(DocumentDetailsMessage::AddEnteredTag)
                    .width(Length::Fill);

                let input_row = Row::new()
                    .push(input)
                    .spacing(space_s)
                    .align_y(Vertical::Center);

                column.push(input_row)
            }
            TagsState::Loading => column.push(
                Row::new()
                    .spacing(space_xs)
                    .align_y(Vertical::Center)
                    .push(
                        widget::icon::from_name("content-loading-symbolic")
                            .size(16)
                            .icon(),
                    )
                    .push(text(fl!("file-details-loading-tags"))),
            ),
            _ => column.push(text("Failed to load tags")),
        };

        column.into()
    }
}
