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
use cosmic::widget::row;
use cosmic::widget::text;

use crate::ICON_SIZE;
use crate::aggregator::Document;
use crate::aggregator::DocumentSource;
use crate::app::ContextView;
use crate::client::ClientSelector;
use crate::component::provided_state::ProvidedState;
use crate::component::provided_state::ProvidedStateMessage;
use crate::component::tag_editor::Orientation;
use crate::component::tag_editor::TagEditor;
use crate::component::tag_editor::TagEditorMessage;
use crate::component::tag_editor::TagEditorOutput;
use crate::cosmic_ext::ActionExt;
use crate::document_provider::DocumentProvider;
use crate::fl;
use crate::layout::layout;
use crate::state::LoadedState;

pub struct DocumentDetails {
    document: Document,
    document_provider: Arc<DocumentProvider>,
    all_clients: ProvidedState<Arc<DocumentProvider>, Vec<ClientSelector>>,
    tag_editor: TagEditor<Arc<DocumentProvider>>,
    editing_sources: bool,
    pending_source_deletion: Option<DocumentSource>,
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
    ToggleEditSources,
    RequestDeleteSource(DocumentSource),
    ConfirmDeleteSource,
    CancelDeleteSource,
    DeleteSource(DocumentSource),
    SourceDeleted(Result<(), String>),
    AllClients(ProvidedStateMessage<Vec<ClientSelector>>),
    SendToClient(ClientSelector),
    SentToClient(Result<(), String>),

    // Message intended for the parent module
    Out(DocumentDetailsOutput),
}

impl From<ProvidedStateMessage<Vec<ClientSelector>>> for DocumentDetailsMessage {
    fn from(value: ProvidedStateMessage<Vec<ClientSelector>>) -> Self {
        Self::AllClients(value)
    }
}

impl DocumentDetails {
    pub fn new(
        document: Document,
        document_provider: Arc<DocumentProvider>,
    ) -> (Self, Task<Action<DocumentDetailsMessage>>) {
        let initial_tags = document.metadata.tags.clone();

        let (tag_editor, tag_editor_task) = TagEditor::new(
            document_provider.clone(),
            initial_tags,
            Orientation::Horizontal,
            fl!("tag-editor-select-tag"),
            fl!("tag-editor-enter"),
            fl!("tag-editor-no-tags"),
            fl!("tag-editor-remove-tag"),
        );

        let (all_clients, init_all_clients) = ProvidedState::new(document_provider.clone());
        let file_details = DocumentDetails {
            document,
            document_provider,
            all_clients,
            tag_editor,
            editing_sources: false,
            pending_source_deletion: None,
        };

        (
            file_details,
            task::batch([
                tag_editor_task.map(|action| action.map(DocumentDetailsMessage::TagEditor)),
                init_all_clients.map(ActionExt::map_into),
            ]),
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

        // Main layout using settings view_column
        let mut sections: Vec<Element<'_, DocumentDetailsMessage>> = vec![
            header.into(),
            basic_info_section.into(),
            technical_section.into(),
            tags_section.into(),
        ];

        // Show confirmation dialog just above the sources section
        if let Some(source) = &self.pending_source_deletion {
            let dialog = widget::dialog()
                .title(fl!("document-details-delete-source-confirm-title"))
                .body(fl!("document-details-delete-source-confirm-body"))
                .icon(widget::icon::from_name("dialog-warning-symbolic").size(64))
                .control(
                    widget::text::monotext(&source.path)
                        .apply(widget::container)
                        .class(theme::Container::Card)
                        .padding(space_s)
                        .width(Length::Fill),
                )
                .primary_action(
                    widget::button::destructive(fl!(
                        "document-details-delete-source-confirm-delete"
                    ))
                    .on_press(DocumentDetailsMessage::ConfirmDeleteSource),
                )
                .secondary_action(
                    widget::button::standard(fl!("document-details-delete-source-confirm-cancel"))
                        .on_press(DocumentDetailsMessage::CancelDeleteSource),
                );

            sections.push(
                row()
                    .push(widget::horizontal_space())
                    .push(dialog.width(Length::FillPortion(10)))
                    .push(widget::horizontal_space())
                    .into(),
            );
        }

        sections.extend(self.sources_view());

        let content = widget::settings::view_column(sections);

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
        ContextView {
            title: fl!("document-details-send-to"),
            content: widget::horizontal_space().into(),
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
            DocumentDetailsMessage::AllClients(message) => {
                self.all_clients.update(message).map(ActionExt::map_into)
            }
            DocumentDetailsMessage::SendToClient(client) => {
                let document = self.document.clone();
                let document_provider = self.document_provider.clone();
                task::future(async move {
                    let result = document_provider
                        .send_document_to_client(document, client)
                        .await
                        .map_err(|err| format!("{err}"));
                    DocumentDetailsMessage::SentToClient(result)
                })
            }
            DocumentDetailsMessage::SentToClient(result) => match result {
                Ok(()) => task::message(DocumentDetailsMessage::RefreshDocument),
                Err(err) => {
                    tracing::error!("Failed to send document to client: {err}");
                    Task::none()
                }
            },
            DocumentDetailsMessage::ToggleEditSources => {
                self.editing_sources = !self.editing_sources;
                self.pending_source_deletion = None;
                Task::none()
            }
            DocumentDetailsMessage::RequestDeleteSource(source) => {
                self.pending_source_deletion = Some(source);
                Task::none()
            }
            DocumentDetailsMessage::ConfirmDeleteSource => {
                if let Some(source) = self.pending_source_deletion.take() {
                    task::message(DocumentDetailsMessage::DeleteSource(source))
                } else {
                    Task::none()
                }
            }
            DocumentDetailsMessage::CancelDeleteSource => {
                self.pending_source_deletion = None;
                Task::none()
            }
            DocumentDetailsMessage::DeleteSource(source) => {
                let metadata = self.document.metadata.clone();
                let document_provider = self.document_provider.clone();
                task::future(async move {
                    let result = document_provider
                        .delete_document_source(source, metadata)
                        .await
                        .map_err(|err| format!("{err}"));
                    DocumentDetailsMessage::SourceDeleted(result)
                })
            }
            DocumentDetailsMessage::SourceDeleted(result) => match result {
                Ok(()) => task::message(DocumentDetailsMessage::RefreshDocument),
                Err(err) => {
                    tracing::error!("Failed to delete source: {err}");
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

    // Sources sections showing all locations where this document exists
    fn sources_view(&self) -> Vec<Element<'_, DocumentDetailsMessage>> {
        let cosmic_theme::Spacing {
            space_xxs,
            space_xs,
            space_s,
            ..
        } = theme::active().cosmic().spacing;

        let mut sections = Vec::new();

        // Edit toggle button, right-aligned
        let edit_button = if self.editing_sources {
            widget::button::icon(widget::icon::from_name("edit-undo-symbolic").size(ICON_SIZE))
                .on_press(DocumentDetailsMessage::ToggleEditSources)
                .tooltip(fl!("document-details-done-editing-sources"))
        } else {
            widget::button::icon(widget::icon::from_name("edit-symbolic").size(ICON_SIZE))
                .on_press(DocumentDetailsMessage::ToggleEditSources)
                .tooltip(fl!("document-details-edit-sources"))
        };

        let mut sources_section =
            widget::settings::section().title(fl!("document-details-sources"));

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

        for source in &sources {
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

            let mut source_row = Row::new()
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

            // Show delete button in edit mode for sources where the client has multiple entries
            if self.editing_sources {
                let client_source_count =
                    sources.iter().filter(|s| s.client == source.client).count();
                if client_source_count > 1 {
                    source_row = source_row.push(
                        widget::button::icon(
                            widget::icon::from_name("list-remove-symbolic").size(ICON_SIZE),
                        )
                        .class(theme::Button::Destructive)
                        .on_press(DocumentDetailsMessage::RequestDeleteSource(
                            (*source).clone(),
                        ))
                        .tooltip(fl!("document-details-delete-source")),
                    );
                }
            }

            sources_section =
                sources_section.add(widget::settings::item_row(vec![source_row.into()]));
        }

        if self.document.sources.is_empty() {
            sources_section = sources_section.add(widget::settings::item_row(vec![
                text(fl!("document-details-no-sources")).into(),
            ]));
        }

        sources_section = sources_section.add(widget::settings::item_row(vec![
            Row::new()
                .push(widget::horizontal_space())
                .push(edit_button)
                .into(),
        ]));

        sections.push(sources_section.into());

        // In edit mode, show "Send To" buttons for clients where the document is missing
        if self.editing_sources
            && let LoadedState::Loaded(all_clients) = &self.all_clients.state
        {
            let document_clients = self.document.get_client_selectors();
            let missing_at: Vec<_> = all_clients
                .iter()
                .filter(|client| !document_clients.contains(client))
                .collect();

            if !missing_at.is_empty() {
                let mut send_to_section =
                    widget::settings::section().title(fl!("document-details-send-to-missing"));

                for client in missing_at {
                    let (icon_name, label, button_label) = match client {
                        ClientSelector::Local => (
                            "computer-symbolic",
                            fl!("document-details-source-local"),
                            fl!("document-details-download-to-local"),
                        ),
                        ClientSelector::Remote(url) => (
                            "network-server-symbolic",
                            url.host_str().unwrap_or("Remote").to_string(),
                            fl!(
                                "document-details-upload-to",
                                host = url.host_str().unwrap_or("Remote")
                            ),
                        ),
                    };
                    send_to_section = send_to_section.add(
                        widget::settings::item::builder(label)
                            .icon(widget::icon::from_name(icon_name).size(ICON_SIZE))
                            .control(
                                widget::button::suggested(button_label)
                                    .on_press(DocumentDetailsMessage::SendToClient(client.clone())),
                            ),
                    );
                }

                sections.push(send_to_section.into());
            }
        }

        sections
    }
}
