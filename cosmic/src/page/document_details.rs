use std::path::Path;
use std::sync::Arc;

use archive_organizer::api::ReadingStatus;
use cosmic::Action;
use cosmic::Apply;
use cosmic::Element;
use cosmic::Task;
use cosmic::cosmic_theme;
use cosmic::iced::Length;
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::alignment::Vertical;
use cosmic::task;
use cosmic::theme;
use cosmic::widget;
use cosmic::widget::Column;
use cosmic::widget::Row;
use cosmic::widget::text;

use crate::ICON_SIZE;
use crate::aggregator::Document;
use crate::app::ContextView;
use crate::component::tag_editor::Orientation;
use crate::component::tag_editor::TagEditor;
use crate::component::tag_editor::TagEditorMessage;
use crate::component::tag_editor::TagEditorOutput;
use crate::cosmic_ext::ActionExt;
use crate::document_provider::DocumentProvider;
use crate::fl;
use crate::layout::layout;

pub struct DocumentDetails {
    document: Document,
    document_provider: Arc<DocumentProvider>,
    tag_editor: TagEditor<Arc<DocumentProvider>>,
}

#[derive(Debug, Clone)]
pub enum DocumentDetailsOutput {
    Close(String), // Fingerprint
    RefreshDocument(Document),
}

#[derive(Debug, Clone)]
pub enum DocumentDetailsMessage {
    TagEditor(TagEditorMessage),
    TagsAdded(Result<Vec<String>, String>),
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
        document_provider: Arc<DocumentProvider>,
    ) -> (Self, Task<Action<DocumentDetailsMessage>>) {
        let initial_tags = document.metadata.tags.clone();
        let document_provider_clone = document_provider.clone();

        let (tag_editor, tag_editor_task) = TagEditor::new(
            document_provider_clone.clone(),
            initial_tags,
            Orientation::Horizontal,
            fl!("tag-editor-select-tag"),
            fl!("tag-editor-enter"),
            fl!("tag-editor-no-tags"),
            fl!("tag-editor-remove-tag"),
        );

        let file_details = DocumentDetails {
            document,
            document_provider,
            tag_editor,
        };

        (
            file_details,
            tag_editor_task.map(|action| action.map(DocumentDetailsMessage::TagEditor)),
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
                        widget::button::icon(
                            widget::icon::from_name("document-viewer-symbolic").size(ICON_SIZE),
                        )
                        .on_press(DocumentDetailsMessage::OpenDocument)
                        .tooltip(fl!("document-details-open-file")),
                    )
                    .push(
                        widget::button::icon(
                            widget::icon::from_name("window-close-symbolic").size(ICON_SIZE),
                        )
                        .on_press(DocumentDetailsMessage::Out(DocumentDetailsOutput::Close(
                            self.document.metadata.fingerprint.clone(),
                        )))
                        .tooltip(fl!("document-details-close")),
                    ),
            );

        // Build settings sections
        let basic_info_section = widget::settings::section()
            .title(fl!("document-details-basic-info"))
            .add(
                widget::settings::item::builder(fl!("document-details-filename"))
                    .icon(widget::icon::from_name("document-open-symbolic").size(ICON_SIZE))
                    .control(text(filename)),
            )
            .add(
                widget::settings::item::builder(fl!("document-details-folder"))
                    .icon(widget::icon::from_name("folder-symbolic").size(ICON_SIZE))
                    .control(text(folder)),
            )
            .add(
                widget::settings::item::builder(fl!("document-details-type"))
                    .icon(widget::icon::from_name("document-properties-symbolic").size(ICON_SIZE))
                    .control(text(self.document.metadata.type_.as_str())),
            )
            .add(
                widget::settings::item::builder(fl!("document-details-size"))
                    .icon(widget::icon::from_name("document-properties-symbolic").size(ICON_SIZE))
                    .control(text(
                        self.format_file_size(self.document.metadata.size.into()),
                    )),
            )
            .add(
                widget::settings::item::builder(fl!("document-details-status"))
                    .icon(widget::icon::from_name("document-properties-symbolic").size(ICON_SIZE))
                    .control(
                        cosmic::iced::widget::pick_list(
                            [
                                ReadingStatus::Unread,
                                ReadingStatus::Reading,
                                ReadingStatus::Read,
                            ],
                            Some(self.document.metadata.status),
                            DocumentDetailsMessage::UpdateReadingStatus,
                        )
                        .placeholder(fl!("document-details-select-status")),
                    ),
            );

        let technical_section = widget::settings::section()
            .title(fl!("document-details-technical"))
            .add(
                widget::settings::item::builder(fl!("document-details-fingerprint"))
                    .icon(widget::icon::from_name("auth-fingerprint-symbolic").size(ICON_SIZE))
                    .control(text(&self.document.metadata.fingerprint)),
            );

        let tags_section = widget::settings::section()
            .title(fl!("document-details-tags"))
            .add(widget::settings::item_row(vec![
                widget::icon::from_name("starred-symbolic")
                    .size(ICON_SIZE)
                    .into(),
                self.tag_editor
                    .view()
                    .map(DocumentDetailsMessage::TagEditor)
                    .apply(widget::container)
                    .width(Length::Fill)
                    .into(),
            ]));

        let sources_section = widget::settings::section()
            .title(fl!("document-details-sources"))
            .add(self.sources_view());

        // Main layout using settings view_column
        let content = widget::settings::view_column(vec![
            header.into(),
            basic_info_section.into(),
            technical_section.into(),
            tags_section.into(),
            sources_section.into(),
        ]);

        // Wrap content in a scrollable container
        layout(content)
            .apply(widget::scrollable::vertical)
            .apply(widget::container)
            .height(Length::Fill)
            .align_x(Horizontal::Center)
            .align_y(Vertical::Top)
            .into()
    }

    pub fn view_context(&self) -> ContextView<'_, DocumentDetailsMessage> {
        // let document_clients = self.document.get_client_selectors();
        // let (_, missing_at) = self
        //     .document_provider
        //     .get_client_selectors()
        //     .await
        //     .into_iter()
        //     .partition(|client| document_clients.contains(client));

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
            DocumentDetailsMessage::TagEditor(tag_msg) => {
                // Handle output messages from tag editor
                match tag_msg {
                    TagEditorMessage::Out(message) => match message {
                        TagEditorOutput::TagAdded(tag) => self.add_tag(tag),
                        TagEditorOutput::TagRemoved(tag) => self.remove_tag(tag),
                        TagEditorOutput::TagsUpdated(_) => {
                            // This is handled via TagAdded/TagRemoved
                            Task::none()
                        }
                    },
                    tag_msg => self
                        .tag_editor
                        .update(tag_msg)
                        .map(|action| action.map(DocumentDetailsMessage::TagEditor)),
                }
            }
            DocumentDetailsMessage::UpdateReadingStatus(status) => {
                let mut updated_document = self.document.clone();
                updated_document.metadata.status = status;
                let document_provider = self.document_provider.clone();

                task::future(async move {
                    let result = document_provider
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
                let document_provider = self.document_provider.clone();
                task::future(async move {
                    if let Err(e) = document_provider.open_document(document).await {
                        tracing::error!("Failed to open file: {e}");
                    }
                    DocumentDetailsMessage::RefreshDocument
                })
            }
            DocumentDetailsMessage::TagsAdded(result) => {
                match result {
                    Ok(_tags) => {
                        // Refresh the file to get updated tags
                        return task::message(DocumentDetailsMessage::RefreshDocument);
                    }
                    Err(err) => {
                        tracing::warn!("Failed to add tag: {}", err);
                    }
                }
                task::none()
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
                let fingerprint = self.document.metadata.fingerprint.clone();
                let document_provider = self.document_provider.clone();

                task::future(async move {
                    let result = document_provider
                        .get_document(&fingerprint)
                        .await
                        .map_err(|err| format!("{err}"))
                        .and_then(|document| {
                            document.ok_or_else(|| {
                                fl!("document-details-document-no-longer-accessible")
                            })
                        });
                    DocumentDetailsMessage::DocumentRefreshed(result)
                })
            }
            DocumentDetailsMessage::DocumentRefreshed(result) => match result {
                Ok(document) => {
                    self.document = document.clone();
                    // Update the tag editor with the new tags
                    let set_tags_task = self
                        .tag_editor
                        .update(TagEditorMessage::SetTags(document.metadata.tags.clone()))
                        .map(|action| action.map(DocumentDetailsMessage::TagEditor));
                    task::batch(vec![
                        set_tags_task,
                        task::message(DocumentDetailsMessage::Out(
                            DocumentDetailsOutput::RefreshDocument(document),
                        )),
                    ])
                }
                Err(err) => {
                    tracing::warn!("Failed to refresh file: {}", err);
                    Task::none()
                }
            },
        }
    }

    fn add_tag(&mut self, tag: String) -> Task<Action<DocumentDetailsMessage>> {
        let document = self.document.clone();
        let document_provider = self.document_provider.clone();

        task::future(async move {
            match document_provider.add_document_tags(document, &[tag]).await {
                Ok(tags) => DocumentDetailsMessage::TagsAdded(Ok(tags)),
                Err(err) => DocumentDetailsMessage::TagsAdded(Err(format!("{err}"))),
            }
        })
        .chain(task::message(DocumentDetailsMessage::RefreshDocument))
    }

    fn remove_tag(&mut self, tag: String) -> Task<Action<DocumentDetailsMessage>> {
        let document = self.document.clone();
        let document_provider = self.document_provider.clone();

        task::future(async move {
            let result = document_provider
                .delete_document_tags(document, &[tag])
                .await
                .map_err(|err| format!("{err}"));
            DocumentDetailsMessage::TagsRemoved(result)
        })
        .chain(task::message(DocumentDetailsMessage::RefreshDocument))
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

    // Sources view showing all locations where this document exists
    fn sources_view(&self) -> Element<'_, DocumentDetailsMessage> {
        let cosmic_theme::Spacing {
            space_xxs,
            space_xs,
            space_s,
            ..
        } = theme::active().cosmic().spacing;

        let mut column = Column::new().spacing(space_s);

        // Sort sources to show local first, then remotes
        let mut sources: Vec<_> = self.document.sources.iter().collect();
        sources.sort_by(|a, b| match (&a.client, &b.client) {
            (crate::client::ClientSelector::Local, crate::client::ClientSelector::Local) => {
                a.path.cmp(&b.path)
            }
            (crate::client::ClientSelector::Local, _) => std::cmp::Ordering::Less,
            (_, crate::client::ClientSelector::Local) => std::cmp::Ordering::Greater,
            (
                crate::client::ClientSelector::Remote(url_a),
                crate::client::ClientSelector::Remote(url_b),
            ) => url_a.cmp(url_b).then(a.path.cmp(&b.path)),
        });

        for source in sources {
            let (icon_name, source_label) = match &source.client {
                crate::client::ClientSelector::Local => {
                    ("computer-symbolic", fl!("document-details-source-local"))
                }
                crate::client::ClientSelector::Remote(url) => (
                    "network-server-symbolic",
                    url.host_str().unwrap_or("Remote").to_string(),
                ),
            };

            let source_path = Path::new(&source.path);
            let folder = source_path.parent().and_then(|p| p.to_str()).unwrap_or("");
            let filename = source_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&source.path);

            let source_row = Row::new()
                .spacing(space_s)
                .align_y(Vertical::Center)
                .push(widget::icon::from_name(icon_name).size(ICON_SIZE).icon())
                .push(
                    Column::new()
                        .spacing(space_xxs)
                        .push(
                            Row::new()
                                .spacing(space_xs)
                                .push(
                                    widget::container(text(source_label).size(12))
                                        .class(theme::Container::Primary)
                                        .padding([2, 6]),
                                )
                                .push(text(filename).width(Length::Fill)),
                        )
                        .push(text(folder).size(12))
                        .width(Length::Fill),
                );

            column = column.push(source_row);
        }

        if self.document.sources.is_empty() {
            column = column.push(text(fl!("document-details-no-sources")));
        }

        column.into()
    }
}
