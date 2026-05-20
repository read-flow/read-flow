use std::path::Path;
use std::sync::Arc;

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
use read_flow_core::api::ReadingStatus;
use read_flow_core::db::datasource::DbClient;
use read_flow_core::db::models::ContentMetadata;
use read_flow_core::db::models::DocumentType;
use read_flow_core::scan::metadata;
use read_flow_core::scan::metadata::ExtractedMetadata;
use strum::IntoEnumIterator;

use crate::ApplicationModule;
use crate::ICON_SIZE;
use crate::aggregator::Document;
use crate::aggregator::DocumentSource;
use crate::aggregator::UserMeta;
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
use crate::page::Page;
use crate::state::LoadedState;

pub struct DocumentDetails {
    document: Document,
    document_provider: Arc<DocumentProvider>,
    application_module: Arc<ApplicationModule>,
    all_clients: ProvidedState<Arc<DocumentProvider>, Vec<ClientSelector>>,
    tag_editor: TagEditor<Arc<DocumentProvider>>,
    editing_sources: bool,
    pending_source_deletion: Option<DocumentSource>,
    format_metadata: Option<ExtractedMetadata>,
    format_metadata_loading: bool,
    editing_user_meta: bool,
    user_meta_draft: UserMeta,
    user_meta_authors_text: String,
}

#[derive(Debug, Clone)]
pub enum DocumentDetailsOutput {
    Close(String), // Fingerprint
    RefreshDocument(Document),
    OpenDocument(Document),
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
    CopyPath(String),
    ToggleEditSources,
    RequestDeleteSource(DocumentSource),
    ConfirmDeleteSource,
    CancelDeleteSource,
    DeleteSource(DocumentSource),
    SourceDeleted(Result<(), String>),
    AllClients(ProvidedStateMessage<Vec<ClientSelector>>),
    SendToClient(ClientSelector),
    SentToClient(Result<(), String>),
    LoadFormatMetadata,
    FormatMetadataLoaded(Result<ExtractedMetadata, String>),

    EditUserMeta,
    CancelUserMeta,
    SaveUserMeta,
    UserMetaSaved(Result<(), String>),
    UserMetaTitleChanged(String),
    UserMetaSubtitleChanged(String),
    UserMetaDocTypeChanged(Option<DocumentType>),
    UserMetaDescriptionChanged(String),
    UserMetaAuthorsChanged(String),

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
        application_module: Arc<ApplicationModule>,
    ) -> (Self, Task<Action<DocumentDetailsMessage>>) {
        let initial_tags = document.metadata.tags.clone();

        let (tag_editor, tag_editor_task) = TagEditor::new(
            document_provider.clone(),
            initial_tags,
            Orientation::Horizontal,
            fl!("tag-editor-select-tag"),
            fl!("tag-editor-no-tags"),
            fl!("tag-editor-remove-tag"),
        );

        let (all_clients, init_all_clients) = ProvidedState::new(document_provider.clone());
        let initial_user_meta = document.user_meta.clone();
        let initial_authors_text = initial_user_meta
            .authors
            .as_deref()
            .unwrap_or(&[])
            .join(", ");
        let file_details = DocumentDetails {
            document,
            document_provider,
            application_module,
            all_clients,
            tag_editor,
            editing_sources: false,
            pending_source_deletion: None,
            format_metadata: None,
            format_metadata_loading: false,
            editing_user_meta: false,
            user_meta_draft: initial_user_meta,
            user_meta_authors_text: initial_authors_text,
        };

        (
            file_details,
            task::batch([
                tag_editor_task.map(|action| action.map(DocumentDetailsMessage::TagEditor)),
                init_all_clients.map(ActionExt::map_into),
                task::message(DocumentDetailsMessage::LoadFormatMetadata),
            ]),
        )
    }

    pub fn display_name(&self) -> String {
        self.document
            .user_meta
            .title
            .clone()
            .unwrap_or_else(|| {
                Path::new(&self.document.sources.iter().next().unwrap().path)
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Unknown")
                    .to_string()
            })
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

    fn user_meta_section_view(&self) -> Element<'_, DocumentDetailsMessage> {
        let cosmic_theme::Spacing { space_s, .. } = theme::active().cosmic().spacing;

        let edit_button = if self.editing_user_meta {
            Row::new()
                .spacing(space_s)
                .push(
                    widget::button::standard(fl!("document-details-user-meta-save"))
                        .on_press(DocumentDetailsMessage::SaveUserMeta),
                )
                .push(
                    widget::button::standard(fl!("document-details-user-meta-cancel"))
                        .on_press(DocumentDetailsMessage::CancelUserMeta),
                )
        } else {
            Row::new().push(
                widget::button::icon(widget::icon::from_name("edit-symbolic").size(ICON_SIZE))
                    .on_press(DocumentDetailsMessage::EditUserMeta)
                    .tooltip(fl!("document-details-user-meta-edit")),
            )
        };

        let section = widget::settings::section().header(widget::settings::item_row(vec![
            text::heading(fl!("document-details-user-meta-section")).into(),
            widget::space::horizontal().into(),
            edit_button.into(),
        ]));

        let meta = if self.editing_user_meta {
            &self.user_meta_draft
        } else {
            &self.document.user_meta
        };

        let doc_type_options: Vec<DocumentType> = DocumentType::iter().collect();

        let type_control: Element<'_, DocumentDetailsMessage> = if self.editing_user_meta {
            cosmic::iced::widget::pick_list(
                doc_type_options,
                meta.document_type,
                |t: DocumentType| DocumentDetailsMessage::UserMetaDocTypeChanged(Some(t)),
            )
            .placeholder(fl!("document-details-user-meta-type-none"))
            .into()
        } else {
            text(
                meta.document_type
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| fl!("document-details-user-meta-type-none")),
            )
            .into()
        };

        let title_control: Element<'_, DocumentDetailsMessage> = if self.editing_user_meta {
            widget::text_input(
                self.format_metadata
                    .as_ref()
                    .and_then(|m| m.title.as_deref())
                    .unwrap_or(""),
                meta.title.as_deref().unwrap_or(""),
            )
            .on_input(DocumentDetailsMessage::UserMetaTitleChanged)
            .into()
        } else {
            text(meta.title.as_deref().unwrap_or("—")).into()
        };

        let subtitle_control: Element<'_, DocumentDetailsMessage> = if self.editing_user_meta {
            widget::text_input("", meta.subtitle.as_deref().unwrap_or(""))
                .on_input(DocumentDetailsMessage::UserMetaSubtitleChanged)
                .into()
        } else {
            text(meta.subtitle.as_deref().unwrap_or("—")).into()
        };

        let authors_display = meta
            .authors
            .as_deref()
            .filter(|a| !a.is_empty())
            .map(|a| a.join(", "))
            .unwrap_or_else(|| "—".to_string());
        let authors_control: Element<'_, DocumentDetailsMessage> = if self.editing_user_meta {
            widget::text_input("", &self.user_meta_authors_text)
                .on_input(DocumentDetailsMessage::UserMetaAuthorsChanged)
                .into()
        } else {
            text(authors_display).into()
        };

        let description_control: Element<'_, DocumentDetailsMessage> = if self.editing_user_meta {
            widget::text_input("", meta.description.as_deref().unwrap_or(""))
                .on_input(DocumentDetailsMessage::UserMetaDescriptionChanged)
                .into()
        } else {
            text(meta.description.as_deref().unwrap_or("—")).into()
        };

        let section = section
            .add(
                widget::settings::item::builder(fl!("document-details-user-meta-type"))
                    .icon(
                        widget::icon::from_name("document-properties-symbolic").size(ICON_SIZE),
                    )
                    .control(type_control),
            )
            .add(
                widget::settings::item::builder(fl!("document-details-user-meta-title"))
                    .icon(
                        widget::icon::from_name("text-x-generic-symbolic").size(ICON_SIZE),
                    )
                    .control(title_control),
            )
            .add(
                widget::settings::item::builder(fl!("document-details-user-meta-subtitle"))
                    .icon(
                        widget::icon::from_name("text-x-generic-symbolic").size(ICON_SIZE),
                    )
                    .control(subtitle_control),
            )
            .add(
                widget::settings::item::builder(fl!("document-details-user-meta-authors"))
                    .icon(
                        widget::icon::from_name("system-users-symbolic").size(ICON_SIZE),
                    )
                    .control(authors_control),
            )
            .add(
                widget::settings::item::builder(fl!("document-details-user-meta-description"))
                    .icon(
                        widget::icon::from_name("accessories-text-editor-symbolic").size(ICON_SIZE),
                    )
                    .control(description_control),
            );

        section.into()
    }

    fn format_metadata_section_view(&self) -> Option<Element<'_, DocumentDetailsMessage>> {
        let meta = self.format_metadata.as_ref()?;
        let mut section =
            widget::settings::section().title(fl!("document-details-metadata-section"));
        let mut has_items = false;

        if let Some(val) = &meta.title {
            section = section.add(
                widget::settings::item::builder(fl!("document-details-metadata-title"))
                    .icon(widget::icon::from_name("text-x-generic-symbolic").size(ICON_SIZE))
                    .control(text(val)),
            );
            has_items = true;
        }
        if !meta.authors.is_empty() {
            section = section.add(
                widget::settings::item::builder(fl!("document-details-metadata-authors"))
                    .icon(widget::icon::from_name("system-users-symbolic").size(ICON_SIZE))
                    .control(text(meta.authors.join(", "))),
            );
            has_items = true;
        }
        if let Some(val) = &meta.publisher {
            section = section.add(
                widget::settings::item::builder(fl!("document-details-metadata-publisher"))
                    .icon(widget::icon::from_name("x-office-address-book-symbolic").size(ICON_SIZE))
                    .control(text(val)),
            );
            has_items = true;
        }
        if let Some(val) = &meta.language {
            section = section.add(
                widget::settings::item::builder(fl!("document-details-metadata-language"))
                    .icon(
                        widget::icon::from_name("preferences-desktop-locale-symbolic")
                            .size(ICON_SIZE),
                    )
                    .control(text(val)),
            );
            has_items = true;
        }
        if let Some(val) = &meta.date {
            section = section.add(
                widget::settings::item::builder(fl!("document-details-metadata-date"))
                    .icon(widget::icon::from_name("x-office-calendar-symbolic").size(ICON_SIZE))
                    .control(text(val)),
            );
            has_items = true;
        }
        if let Some(val) = &meta.identifier {
            section = section.add(
                widget::settings::item::builder(fl!("document-details-metadata-identifier"))
                    .icon(widget::icon::from_name("dialog-information-symbolic").size(ICON_SIZE))
                    .control(text(val)),
            );
            has_items = true;
        }
        if let Some(val) = &meta.subject {
            section = section.add(
                widget::settings::item::builder(fl!("document-details-metadata-subject"))
                    .icon(widget::icon::from_name("edit-find-symbolic").size(ICON_SIZE))
                    .control(text(val)),
            );
            has_items = true;
        }

        if has_items {
            Some(section.into())
        } else {
            None
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
            widget::button::icon(widget::icon::from_name("object-select-symbolic").size(ICON_SIZE))
                .on_press(DocumentDetailsMessage::ToggleEditSources)
                .tooltip(fl!("document-details-done-editing-sources"))
        } else {
            widget::button::icon(widget::icon::from_name("edit-symbolic").size(ICON_SIZE))
                .on_press(DocumentDetailsMessage::ToggleEditSources)
                .tooltip(fl!("document-details-edit-sources"))
        };

        let mut sources_section =
            widget::settings::section().header(widget::settings::item_row(vec![
                text::heading(fl!("document-details-sources")).into(),
                widget::space::horizontal().into(),
                edit_button.into(),
            ]));

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
                )
                .push(
                    widget::button::icon(
                        widget::icon::from_name("edit-copy-symbolic").size(ICON_SIZE),
                    )
                    .on_press(DocumentDetailsMessage::CopyPath(source.path.clone()))
                    .tooltip(fl!("document-details-copy-path")),
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

impl Page for DocumentDetails {
    type Message = DocumentDetailsMessage;

    fn view(&self) -> Element<'_, DocumentDetailsMessage> {
        // Extract filename and folder using std::path
        let path = Path::new(&self.document.sources.iter().next().unwrap().path);

        // Get filename without extension
        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Unknown");

        // Get the folder path
        let folder = path
            .parent()
            .and_then(|parent| parent.to_str())
            .unwrap_or("");

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
        let mut sections: Vec<Element<'_, DocumentDetailsMessage>> =
            vec![basic_info_section.into(), self.user_meta_section_view()];
        if let Some(meta_section) = self.format_metadata_section_view() {
            sections.push(meta_section);
        }
        sections.push(technical_section.into());
        sections.push(tags_section.into());
        sections.extend(self.sources_view());

        let content = widget::settings::view_column(sections);

        // Wrap content in a scrollable container
        layout(content)
            .apply(widget::scrollable::vertical)
            .width(Length::Fill)
            .height(Length::Fill)
            .apply(widget::container)
            .height(Length::Fill)
            .align_x(Horizontal::Center)
            .align_y(Vertical::Top)
            .into()
    }

    fn dialog(&self) -> Option<Element<'_, DocumentDetailsMessage>> {
        let cosmic_theme::Spacing { space_s, .. } = theme::active().cosmic().spacing;

        let source = self.pending_source_deletion.as_ref()?;
        Some(
            widget::dialog()
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
                )
                .into(),
        )
    }

    fn view_header_center(&self) -> Vec<Element<'_, DocumentDetailsMessage>> {
        let header_title = self.document.user_meta.title.as_deref().unwrap_or_else(|| {
            Path::new(&self.document.sources.iter().next().unwrap().path)
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("Unknown")
        });

        vec![
            text::heading(header_title)
                .wrapping(cosmic::iced::widget::text::Wrapping::None)
                .into(),
        ]
    }

    fn view_header_start(&self) -> Vec<Element<'_, DocumentDetailsMessage>> {
        vec![
            widget::button::icon(widget::icon::from_name("go-previous-symbolic").size(ICON_SIZE))
                .on_press(DocumentDetailsMessage::Out(DocumentDetailsOutput::Close(
                    self.document.metadata.fingerprint.clone(),
                )))
                .tooltip(fl!("document-details-close"))
                .into(),
        ]
    }

    fn view_header_end(&self) -> Vec<Element<'_, DocumentDetailsMessage>> {
        vec![
            widget::button::icon(
                widget::icon::from_name("document-viewer-symbolic").size(ICON_SIZE),
            )
            .on_press(DocumentDetailsMessage::OpenDocument)
            .tooltip(fl!("document-details-open-file"))
            .into(),
        ]
    }

    fn update(&mut self, message: DocumentDetailsMessage) -> Task<Action<DocumentDetailsMessage>> {
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
            DocumentDetailsMessage::OpenDocument => task::message(DocumentDetailsMessage::Out(
                DocumentDetailsOutput::OpenDocument(self.document.clone()),
            )),
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
            DocumentDetailsMessage::CopyPath(path) => cosmic::iced::clipboard::write(path),
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
            DocumentDetailsMessage::LoadFormatMetadata => {
                if self.format_metadata_loading {
                    return Task::none();
                }
                self.format_metadata_loading = true;
                let fingerprint = self.document.metadata.fingerprint.clone();
                let type_ = self.document.metadata.type_.clone();
                let path = self.document.sources.iter().next().unwrap().path.clone();
                let application_module = self.application_module.clone();
                Task::perform(
                    async move {
                        let pool = application_module.connection_pool().await;
                        let db_client = DbClient::new(pool);
                        if let Ok(Some(row)) = db_client.get_content_metadata(&fingerprint).await {
                            return Ok(content_metadata_to_extracted(row));
                        }
                        let ext = type_.as_str().to_owned();
                        tokio::task::spawn_blocking(move || {
                            metadata::extract_metadata(std::path::Path::new(&path), &ext)
                                .ok_or_else(|| "unsupported format".to_string())
                        })
                        .await
                        .unwrap_or_else(|_| Err("task panicked".to_string()))
                    },
                    |r| cosmic::action::app(DocumentDetailsMessage::FormatMetadataLoaded(r)),
                )
            }
            DocumentDetailsMessage::FormatMetadataLoaded(result) => {
                self.format_metadata_loading = false;
                match result {
                    Ok(meta) => self.format_metadata = Some(meta),
                    Err(err) => tracing::warn!("Failed to load format metadata: {err}"),
                }
                Task::none()
            }
            DocumentDetailsMessage::EditUserMeta => {
                self.user_meta_draft = self.document.user_meta.clone();
                self.user_meta_authors_text = self
                    .user_meta_draft
                    .authors
                    .as_deref()
                    .unwrap_or(&[])
                    .join(", ");
                self.editing_user_meta = true;
                Task::none()
            }
            DocumentDetailsMessage::CancelUserMeta => {
                self.editing_user_meta = false;
                Task::none()
            }
            DocumentDetailsMessage::SaveUserMeta => {
                // Parse authors from comma-separated text
                let authors: Vec<String> = self
                    .user_meta_authors_text
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect();
                self.user_meta_draft.authors = if authors.is_empty() {
                    None
                } else {
                    Some(authors)
                };
                let draft = self.user_meta_draft.clone();
                let document = self.document.clone();
                let document_provider = self.document_provider.clone();
                self.editing_user_meta = false;
                task::future(async move {
                    let result = document_provider
                        .update_document_metadata(&document, draft)
                        .await
                        .map_err(|e| format!("{e}"));
                    DocumentDetailsMessage::UserMetaSaved(result)
                })
            }
            DocumentDetailsMessage::UserMetaSaved(result) => match result {
                Ok(()) => task::message(DocumentDetailsMessage::RefreshDocument),
                Err(err) => {
                    tracing::error!("Failed to save document metadata: {err}");
                    Task::none()
                }
            },
            DocumentDetailsMessage::UserMetaTitleChanged(val) => {
                self.user_meta_draft.title =
                    if val.is_empty() { None } else { Some(val) };
                Task::none()
            }
            DocumentDetailsMessage::UserMetaSubtitleChanged(val) => {
                self.user_meta_draft.subtitle =
                    if val.is_empty() { None } else { Some(val) };
                Task::none()
            }
            DocumentDetailsMessage::UserMetaDocTypeChanged(val) => {
                self.user_meta_draft.document_type = val;
                Task::none()
            }
            DocumentDetailsMessage::UserMetaDescriptionChanged(val) => {
                self.user_meta_draft.description =
                    if val.is_empty() { None } else { Some(val) };
                Task::none()
            }
            DocumentDetailsMessage::UserMetaAuthorsChanged(val) => {
                self.user_meta_authors_text = val;
                Task::none()
            }
        }
    }
}

fn content_metadata_to_extracted(row: ContentMetadata) -> ExtractedMetadata {
    ExtractedMetadata {
        title: row.title,
        authors: row
            .authors
            .map(|s| s.split(", ").map(str::to_owned).collect())
            .unwrap_or_default(),
        language: row.language,
        publisher: row.publisher,
        identifier: row.identifier,
        date: row.date,
        subject: row.subject,
    }
}
