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
use read_flow_core::db::models::DocumentType;
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
    all_clients: ProvidedState<Arc<DocumentProvider>, Vec<ClientSelector>>,
    tag_editor: TagEditor<Arc<DocumentProvider>>,
    editing_sources: bool,
    pending_source_deletion: Option<DocumentSource>,
    show_open_picker: bool,
    editing_user_meta: bool,
    user_meta_draft: UserMeta,
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
    PickOpenSource(String),
    CancelOpenPicker,
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

    EditUserMeta,
    CancelUserMeta,
    SaveUserMeta,
    UserMetaSaved(Result<(), String>),
    UserMetaTitleChanged(String),
    UserMetaSubtitleChanged(String),
    UserMetaDocTypeChanged(Option<DocumentType>),
    UserMetaDescriptionChanged(String),
    UserMetaAuthorChanged(usize, String),
    UserMetaAuthorRemoved(usize),
    UserMetaAuthorAdded,
    UserMetaLanguageChanged(String),
    UserMetaPublisherChanged(String),
    UserMetaIdentifierChanged(String),
    UserMetaDateChanged(String),
    UserMetaSubjectChanged(String),

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
        _application_module: Arc<ApplicationModule>,
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
        let file_details = DocumentDetails {
            document,
            document_provider,
            all_clients,
            tag_editor,
            editing_sources: false,
            pending_source_deletion: None,
            show_open_picker: false,
            editing_user_meta: false,
            user_meta_draft: initial_user_meta,
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
        self.document.user_meta.title.clone().unwrap_or_else(|| {
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
    fn user_meta_section_view(&self) -> Element<'_, DocumentDetailsMessage> {
        let cosmic_theme::Spacing {
            space_xxs,
            space_xs,
            space_s,
            ..
        } = theme::active().cosmic().spacing;

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

        let editing = self.editing_user_meta;
        let meta = if editing {
            &self.user_meta_draft
        } else {
            &self.document.user_meta
        };

        // In edit mode: always show a text_input.
        // In view mode: show a text label only when the value is non-empty, else skip the row.
        macro_rules! opt_field {
            ($val:expr, $on_input:expr) => {{
                let v: &str = $val.as_deref().unwrap_or("");
                let el: Option<Element<'_, DocumentDetailsMessage>> = if editing {
                    Some(widget::text_input("", v).on_input($on_input).into())
                } else if v.is_empty() {
                    None
                } else {
                    Some(text(v.to_owned()).into())
                };
                el
            }};
        }

        // Document type: pick_list in edit mode, text label (or hidden) in view mode.
        let doc_type_options: Vec<DocumentType> = DocumentType::iter().collect();
        let type_control: Option<Element<'_, DocumentDetailsMessage>> = if editing {
            Some(
                cosmic::iced::widget::pick_list(
                    doc_type_options,
                    meta.document_type,
                    |t: DocumentType| DocumentDetailsMessage::UserMetaDocTypeChanged(Some(t)),
                )
                .placeholder(fl!("document-details-user-meta-type-none"))
                .into(),
            )
        } else {
            meta.document_type
                .map(|t| -> Element<'_, DocumentDetailsMessage> { text(t.to_string()).into() })
        };

        // Authors: comma-separated display in view mode, free-text input in edit mode.
        let authors_control: Option<Element<'_, DocumentDetailsMessage>> = if editing {
            let draft_authors = self.user_meta_draft.authors.as_deref().unwrap_or(&[]);
            let mut col = Column::new().spacing(space_xs);
            for (idx, author) in draft_authors.iter().enumerate() {
                col = col.push(
                    Row::new()
                        .spacing(space_xs)
                        .align_y(Vertical::Center)
                        .push(
                            widget::text_input("", author.as_str())
                                .on_input(move |v| {
                                    DocumentDetailsMessage::UserMetaAuthorChanged(idx, v)
                                })
                                .width(Length::Fill),
                        )
                        .push(
                            widget::button::icon(
                                widget::icon::from_name("list-remove-symbolic").size(ICON_SIZE),
                            )
                            .on_press(DocumentDetailsMessage::UserMetaAuthorRemoved(idx)),
                        ),
                );
            }
            col = col.push(
                widget::button::standard(fl!("document-details-user-meta-authors-add"))
                    .on_press(DocumentDetailsMessage::UserMetaAuthorAdded),
            );
            Some(col.into())
        } else {
            match meta.authors.as_deref().filter(|a| !a.is_empty()) {
                None => None,
                Some(authors) => {
                    let items: Vec<Element<'_, DocumentDetailsMessage>> =
                        authors.iter().map(|a| text(a.as_str()).into()).collect();
                    Some(Column::new().spacing(space_xxs).extend(items).into())
                }
            }
        };

        // Conditionally add each field row.
        let mut section = section;

        macro_rules! add_row {
            ($control:expr, $label:expr, $icon:expr) => {
                if let Some(control) = $control {
                    section = section.add(
                        widget::settings::item::builder($label)
                            .icon(widget::icon::from_name($icon).size(ICON_SIZE))
                            .control(control),
                    );
                }
            };
        }

        add_row!(
            type_control,
            fl!("document-details-user-meta-type"),
            "document-properties-symbolic"
        );
        add_row!(
            opt_field!(meta.title, DocumentDetailsMessage::UserMetaTitleChanged),
            fl!("document-details-user-meta-title"),
            "text-x-generic-symbolic"
        );
        add_row!(
            opt_field!(
                meta.subtitle,
                DocumentDetailsMessage::UserMetaSubtitleChanged
            ),
            fl!("document-details-user-meta-subtitle"),
            "text-x-generic-symbolic"
        );
        add_row!(
            authors_control,
            fl!("document-details-user-meta-authors"),
            "system-users-symbolic"
        );
        add_row!(
            opt_field!(
                meta.description,
                DocumentDetailsMessage::UserMetaDescriptionChanged
            ),
            fl!("document-details-user-meta-description"),
            "accessories-text-editor-symbolic"
        );
        add_row!(
            opt_field!(
                meta.language,
                DocumentDetailsMessage::UserMetaLanguageChanged
            ),
            fl!("document-details-metadata-language"),
            "preferences-desktop-locale-symbolic"
        );
        add_row!(
            opt_field!(
                meta.publisher,
                DocumentDetailsMessage::UserMetaPublisherChanged
            ),
            fl!("document-details-metadata-publisher"),
            "x-office-address-book-symbolic"
        );
        add_row!(
            opt_field!(
                meta.identifier,
                DocumentDetailsMessage::UserMetaIdentifierChanged
            ),
            fl!("document-details-metadata-identifier"),
            "dialog-information-symbolic"
        );
        add_row!(
            opt_field!(meta.date, DocumentDetailsMessage::UserMetaDateChanged),
            fl!("document-details-metadata-date"),
            "x-office-calendar-symbolic"
        );
        add_row!(
            opt_field!(meta.subject, DocumentDetailsMessage::UserMetaSubjectChanged),
            fl!("document-details-metadata-subject"),
            "edit-find-symbolic"
        );

        section.into()
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
                                .push(
                                    widget::container(
                                        text(source.type_.as_str().to_uppercase()).size(12),
                                    )
                                    .class(theme::Container::Card)
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

            if matches!(source.client, ClientSelector::Local) {
                source_row = source_row.push(
                    widget::button::icon(
                        widget::icon::from_name("document-viewer-symbolic").size(ICON_SIZE),
                    )
                    .on_press(DocumentDetailsMessage::PickOpenSource(source.guid.clone()))
                    .tooltip(fl!("document-details-open-file")),
                );
            }

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
        let status_section = widget::settings::section().add(
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
            vec![status_section.into(), self.user_meta_section_view()];
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

        if let Some(source) = &self.pending_source_deletion {
            return Some(
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
                        widget::button::standard(fl!(
                            "document-details-delete-source-confirm-cancel"
                        ))
                        .on_press(DocumentDetailsMessage::CancelDeleteSource),
                    )
                    .into(),
            );
        }

        if self.show_open_picker {
            let local_sources: Vec<&DocumentSource> = self
                .document
                .sources
                .iter()
                .filter(|s| matches!(s.client, ClientSelector::Local))
                .collect();

            return Some(crate::component::source_picker::source_picker_dialog(
                fl!("document-details-open-file"),
                None,
                local_sources,
                DocumentDetailsMessage::PickOpenSource,
                DocumentDetailsMessage::CancelOpenPicker,
            ));
        }

        None
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
        let has_local = self
            .document
            .sources
            .iter()
            .any(|s| matches!(s.client, ClientSelector::Local));
        let btn = widget::button::icon(
            widget::icon::from_name("document-viewer-symbolic").size(ICON_SIZE),
        )
        .tooltip(fl!("document-details-open-file"));
        let btn = if has_local {
            btn.on_press(DocumentDetailsMessage::OpenDocument)
        } else {
            btn
        };
        vec![btn.into()]
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
            DocumentDetailsMessage::OpenDocument => {
                let local_sources: Vec<_> = self
                    .document
                    .sources
                    .iter()
                    .filter(|s| matches!(s.client, ClientSelector::Local))
                    .collect();
                match local_sources.len() {
                    0 => Task::none(),
                    1 => {
                        if let Some(single) = self.document.with_source_guid(&local_sources[0].guid)
                        {
                            task::message(DocumentDetailsMessage::Out(
                                DocumentDetailsOutput::OpenDocument(single),
                            ))
                        } else {
                            Task::none()
                        }
                    }
                    _ => {
                        self.show_open_picker = true;
                        Task::none()
                    }
                }
            }
            DocumentDetailsMessage::PickOpenSource(guid) => {
                self.show_open_picker = false;
                if let Some(single) = self.document.with_source_guid(&guid) {
                    task::message(DocumentDetailsMessage::Out(
                        DocumentDetailsOutput::OpenDocument(single),
                    ))
                } else {
                    Task::none()
                }
            }
            DocumentDetailsMessage::CancelOpenPicker => {
                self.show_open_picker = false;
                Task::none()
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
            DocumentDetailsMessage::EditUserMeta => {
                self.user_meta_draft = self.document.user_meta.clone();
                self.editing_user_meta = true;
                Task::none()
            }
            DocumentDetailsMessage::CancelUserMeta => {
                self.editing_user_meta = false;
                Task::none()
            }
            DocumentDetailsMessage::SaveUserMeta => {
                // Drop empty author entries before saving.
                if let Some(authors) = &mut self.user_meta_draft.authors {
                    authors.retain(|a| !a.trim().is_empty());
                    if authors.is_empty() {
                        self.user_meta_draft.authors = None;
                    }
                }
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
                self.user_meta_draft.title = if val.is_empty() { None } else { Some(val) };
                Task::none()
            }
            DocumentDetailsMessage::UserMetaSubtitleChanged(val) => {
                self.user_meta_draft.subtitle = if val.is_empty() { None } else { Some(val) };
                Task::none()
            }
            DocumentDetailsMessage::UserMetaDocTypeChanged(val) => {
                self.user_meta_draft.document_type = val;
                Task::none()
            }
            DocumentDetailsMessage::UserMetaDescriptionChanged(val) => {
                self.user_meta_draft.description = if val.is_empty() { None } else { Some(val) };
                Task::none()
            }
            DocumentDetailsMessage::UserMetaAuthorChanged(idx, val) => {
                if let Some(authors) = &mut self.user_meta_draft.authors
                    && let Some(author) = authors.get_mut(idx)
                {
                    *author = val;
                }
                Task::none()
            }
            DocumentDetailsMessage::UserMetaAuthorRemoved(idx) => {
                if let Some(authors) = &mut self.user_meta_draft.authors {
                    if idx < authors.len() {
                        authors.remove(idx);
                    }
                    if authors.is_empty() {
                        self.user_meta_draft.authors = None;
                    }
                }
                Task::none()
            }
            DocumentDetailsMessage::UserMetaAuthorAdded => {
                self.user_meta_draft
                    .authors
                    .get_or_insert_with(Vec::new)
                    .push(String::new());
                Task::none()
            }
            DocumentDetailsMessage::UserMetaLanguageChanged(val) => {
                self.user_meta_draft.language = if val.is_empty() { None } else { Some(val) };
                Task::none()
            }
            DocumentDetailsMessage::UserMetaPublisherChanged(val) => {
                self.user_meta_draft.publisher = if val.is_empty() { None } else { Some(val) };
                Task::none()
            }
            DocumentDetailsMessage::UserMetaIdentifierChanged(val) => {
                self.user_meta_draft.identifier = if val.is_empty() { None } else { Some(val) };
                Task::none()
            }
            DocumentDetailsMessage::UserMetaDateChanged(val) => {
                self.user_meta_draft.date = if val.is_empty() { None } else { Some(val) };
                Task::none()
            }
            DocumentDetailsMessage::UserMetaSubjectChanged(val) => {
                self.user_meta_draft.subject = if val.is_empty() { None } else { Some(val) };
                Task::none()
            }
        }
    }
}
