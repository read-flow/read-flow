use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;

use cosmic::Action;
use cosmic::Apply;
use cosmic::Element;
use cosmic::Task;
use cosmic::cosmic_theme;
use cosmic::iced::ContentFit;
use cosmic::iced::Length;
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::alignment::Vertical;
use cosmic::iced::widget::text::Wrapping;
use cosmic::task;
use cosmic::theme;
use cosmic::widget;
use cosmic::widget::Column;
use cosmic::widget::Row;
use cosmic::widget::text;
use cosmic::widget::text_editor;
use read_flow_core::Builder;
use read_flow_core::api::ReadingStatus;
use read_flow_core::db::models::DocumentType;
use strum::IntoEnumIterator;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;

use crate::ApplicationModule;
use crate::ICON_SIZE;
use crate::aggregator::Document;
use crate::aggregator::DocumentMeta;
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
use crate::page::Page;
use crate::page::image_viewer::ViewerImage;
use crate::state::LoadedState;

/// Formats a source's stored `imported_at` (RFC3339, e.g. from `strftime`) for
/// display. Returns `None` for an empty/unparseable value — sources without a
/// known import time (synthetic, or from a remote predating this field)
/// simply don't show a caption rather than showing a bogus date.
fn format_imported_at(imported_at: &str) -> Option<String> {
    if imported_at.is_empty() {
        return None;
    }
    OffsetDateTime::parse(imported_at, &Rfc3339)
        .ok()?
        .format(format_description!("[month repr:short] [day], [year]"))
        .ok()
}

/// @feature: documents.detail_view
pub struct DocumentDetails {
    document: Document,
    document_provider: Arc<DocumentProvider>,
    all_clients: ProvidedState<Arc<DocumentProvider>, Vec<ClientSelector>>,
    tag_editor: TagEditor<Arc<DocumentProvider>>,
    editing_sources: bool,
    pending_source_deletion: Option<DocumentSource>,
    show_open_picker: bool,
    editing_document_meta: bool,
    document_meta_draft: DocumentMeta,
    /// Covers keyed by content fingerprint (all contents loaded on open).
    covers: std::collections::HashMap<String, (cosmic::widget::image::Handle, Vec<u8>)>,
    description_content: text_editor::Content,
}

#[derive(Debug, Clone)]
pub enum DocumentDetailsOutput {
    Close(String), // Fingerprint
    RefreshDocument(Document),
    OpenDocument(Document),
    OpenImageViewer(ViewerImage),
}

#[derive(Debug, Clone)]
pub enum DocumentDetailsMessage {
    TagEditor(TagEditorMessage),
    TagsAdded(Result<Vec<String>, String>),
    TagsRemoved(Result<(), String>),
    RefreshDocument,
    DocumentRefreshed(Result<Document, String>),
    /// @feature: reading.status
    UpdateReadingStatus(ReadingStatus),
    ReadingStatusUpdated(Result<(), String>),
    OpenDocument,
    PickOpenSource(String),
    CancelOpenPicker,
    CopyPath(String),
    ToggleEditSources,
    /// @feature: sources.delete
    RequestDeleteSource(DocumentSource),
    ConfirmDeleteSource,
    CancelDeleteSource,
    DeleteSource(DocumentSource),
    SourceDeleted(Result<(), String>),
    AllClients(ProvidedStateMessage<Vec<ClientSelector>>),
    /// @feature: sources.send_to_client
    SendToClient(ClientSelector),
    SentToClient(Result<(), String>),
    /// @feature: sources.sync_to_all
    SyncToAllSources,
    SyncedToAllSources(Result<(), String>),

    EditDocumentMeta,
    CancelDocumentMeta,
    /// @feature: documents.edit_metadata
    SaveDocumentMeta,
    DocumentMetaSaved(Result<(), String>),
    DocumentMetaTitleChanged(String),
    DocumentMetaSubtitleChanged(String),
    DocumentMetaDocTypeChanged(Option<DocumentType>),
    DescriptionAction(text_editor::Action),
    DocumentMetaAuthorChanged(usize, String),
    DocumentMetaAuthorRemoved(usize),
    DocumentMetaAuthorAdded,
    DocumentMetaLanguageChanged(String),
    DocumentMetaPublisherChanged(String),
    DocumentMetaIdentifierChanged(String),
    DocumentMetaDateChanged(String),
    DocumentMetaSubjectChanged(String),

    CoversLoaded(std::collections::HashMap<String, (cosmic::widget::image::Handle, Vec<u8>)>),
    /// @feature: documents.cover_display
    OpenCover(String),
    /// @feature: documents.select_cover
    SelectCover(String),
    CoverSelected(Result<(), String>),
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
        let initial_tags: Vec<String> = document
            .contents
            .iter()
            .flat_map(|c| c.tags.iter().cloned())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let (tag_editor, tag_editor_task) = TagEditor::new(
            document_provider.clone(),
            initial_tags,
            Orientation::Horizontal,
            fl!("tag-editor-select-tag"),
            fl!("tag-editor-no-tags"),
            fl!("tag-editor-remove-tag"),
        );

        let (all_clients, init_all_clients) = ProvidedState::new(document_provider.clone());
        let initial_document_meta = document.document_meta.clone();

        // Load covers for ALL contents so the selection grid can show each one.
        let fingerprints: Vec<String> = document
            .contents
            .iter()
            .map(|c| c.fingerprint.clone())
            .collect();
        let cover_task = task::future(async move {
            let pool = application_module.connection_pool().await;
            let Ok(mut conn) = pool.acquire().await else {
                return DocumentDetailsMessage::CoversLoaded(std::collections::HashMap::new());
            };
            let mut map = std::collections::HashMap::new();
            for fp in fingerprints {
                if let Ok(Some((data, _))) =
                    read_flow_core::db::dao::get_cover(&mut conn, &fp).await
                {
                    let handle = cosmic::widget::image::Handle::from_bytes(data.clone());
                    map.insert(fp, (handle, data));
                }
            }
            DocumentDetailsMessage::CoversLoaded(map)
        });

        let file_details = DocumentDetails {
            document,
            document_provider,
            all_clients,
            tag_editor,
            editing_sources: false,
            pending_source_deletion: None,
            show_open_picker: false,
            editing_document_meta: false,
            document_meta_draft: initial_document_meta,
            covers: std::collections::HashMap::new(),
            description_content: text_editor::Content::new(),
        };

        (
            file_details,
            task::batch([
                tag_editor_task.map(|action| action.map(DocumentDetailsMessage::TagEditor)),
                init_all_clients.map(ActionExt::map_into),
                cover_task,
            ]),
        )
    }

    pub fn display_name(&self) -> String {
        self.document
            .document_meta
            .title
            .clone()
            .unwrap_or_else(|| {
                let path = self
                    .document
                    .contents
                    .first()
                    .and_then(|c| c.sources.first())
                    .map(|s| s.path.as_str())
                    .unwrap_or("Unknown");
                Path::new(path)
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Unknown")
                    .to_string()
            })
    }

    /// Cover to show in the hero: the document's selected cover, falling back to
    /// the first content's own cover. Shared by the read-only hero and the
    /// edit-mode hero row so the thumbnail stays visible while editing.
    fn selected_cover(&self) -> Option<(&cosmic::widget::image::Handle, String)> {
        self.document
            .document_meta
            .selected_cover_fingerprint
            .as_ref()
            .or_else(|| self.document.contents.first().map(|c| &c.fingerprint))
            .and_then(|fp| self.covers.get(fp).map(|(h, _)| (h, fp.clone())))
    }

    /// Cover image (if any) alongside arbitrary right-hand content. Used both for
    /// the read-only hero (title/subtitle/authors/description) and, in edit mode,
    /// for the cover paired with the metadata edit form.
    fn hero_row<'a>(
        &'a self,
        cover: Option<(&'a cosmic::widget::image::Handle, String)>,
        cover_size: (f32, f32),
        right: Element<'a, DocumentDetailsMessage>,
    ) -> Element<'a, DocumentDetailsMessage> {
        let cosmic_theme::Spacing {
            space_m,
            space_s,
            space_xl,
            ..
        } = theme::active().cosmic().spacing;

        let spacing = if self.editing_document_meta {
            // No spacing and padding in edit mode
            0
        } else {
            space_m
        };

        let mut hero_row = Row::new().spacing(spacing).align_y(Vertical::Top);

        if let Some((handle, fp)) = cover {
            let (width, height) = cover_size;
            let img = widget::image(handle.clone())
                .width(Length::Fixed(width))
                .height(Length::Fixed(height))
                .content_fit(ContentFit::Contain);
            hero_row = hero_row.push(
                Column::new().push(widget::space().height(space_xl)).push(
                    widget::button::custom(img)
                        .on_press(DocumentDetailsMessage::OpenCover(fp))
                        .padding(0),
                ),
            );
        }

        hero_row = hero_row.push_maybe(
            self.editing_document_meta
                .then_some(widget::space().width(Length::Fixed(space_s.into()))),
        );

        hero_row = hero_row.push(widget::container(right).width(Length::Fill));

        widget::container(hero_row)
            .padding(spacing)
            .width(Length::Fill)
            .into()
    }

    /// An edit-mode field: a compact icon+label line above a full-width control,
    /// instead of a label/control row that would split the row 50/50.
    fn stacked_field<'a>(
        icon: &'static str,
        label: impl Into<std::borrow::Cow<'a, str>> + 'a,
        control: Element<'a, DocumentDetailsMessage>,
    ) -> Element<'a, DocumentDetailsMessage> {
        let cosmic_theme::Spacing { space_xxxs, .. } = theme::active().cosmic().spacing;

        widget::settings::item_row(vec![
            widget::icon::from_name(icon).size(ICON_SIZE).into(),
            Column::new()
                .spacing(space_xxxs)
                .push(text::caption(label))
                .push(control)
                .width(Length::Fill)
                .into(),
        ])
        .align_y(Vertical::Top)
        .into()
    }

    fn hero_section(&self) -> Option<Element<'_, DocumentDetailsMessage>> {
        let cosmic_theme::Spacing { space_xs, .. } = theme::active().cosmic().spacing;

        let meta = &self.document.document_meta;
        let cover = self.selected_cover();

        let has_text = meta.title.is_some()
            || meta.subtitle.is_some()
            || meta.authors.as_deref().is_some_and(|a| !a.is_empty())
            || meta.description.is_some();

        if cover.is_none() && !has_text {
            return None;
        }

        let mut text_col = Column::new().spacing(space_xs);

        if let Some(title) = meta.title.as_deref() {
            text_col = text_col.push(text::title1(title).wrapping(Wrapping::Word));
        }
        if let Some(subtitle) = meta.subtitle.as_deref() {
            text_col = text_col.push(text::title4(subtitle));
        }
        if let Some(authors) = meta.authors.as_deref().filter(|a| !a.is_empty()) {
            text_col = text_col.push(text::heading(authors.join(", ")));
        }
        if let Some(desc) = meta.description.as_deref() {
            text_col = text_col
                .push(widget::divider::horizontal::light())
                .push(text::body(desc).wrapping(Wrapping::Word));
        }

        Some(self.hero_row(cover, (200.0, 300.0), text_col.into()))
    }

    /// @feature: tags.add
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

    /// @feature: tags.remove
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
    fn document_meta_section_view(&self) -> Element<'_, DocumentDetailsMessage> {
        let cosmic_theme::Spacing {
            space_xs, space_s, ..
        } = theme::active().cosmic().spacing;

        let edit_button = if self.editing_document_meta {
            Row::new()
                .spacing(space_s)
                .push(
                    widget::button::standard(fl!("document-details-document-meta-save"))
                        .on_press(DocumentDetailsMessage::SaveDocumentMeta),
                )
                .push(
                    widget::button::standard(fl!("document-details-document-meta-cancel"))
                        .on_press(DocumentDetailsMessage::CancelDocumentMeta),
                )
        } else {
            Row::new().push(
                widget::button::icon(widget::icon::from_name("edit-symbolic").size(ICON_SIZE))
                    .on_press(DocumentDetailsMessage::EditDocumentMeta)
                    .tooltip(fl!("document-details-document-meta-edit")),
            )
        };

        let section = widget::settings::section().header(widget::settings::item_row(vec![
            text::heading(fl!("document-details-document-meta-section")).into(),
            widget::space::horizontal().into(),
            edit_button.into(),
        ]));

        let editing = self.editing_document_meta;
        let meta = if editing {
            &self.document_meta_draft
        } else {
            &self.document.document_meta
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

        // In edit mode: show a text_input. In view mode: hidden (shown in the hero section instead).
        macro_rules! editing_only_field {
            ($val:expr, $on_input:expr) => {{
                let v: &str = $val.as_deref().unwrap_or("");
                let el: Option<Element<'_, DocumentDetailsMessage>> = if editing {
                    Some(widget::text_input("", v).on_input($on_input).into())
                } else {
                    None
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
                    |t: DocumentType| DocumentDetailsMessage::DocumentMetaDocTypeChanged(Some(t)),
                )
                .placeholder(fl!("document-details-document-meta-type-none"))
                .into(),
            )
        } else {
            meta.document_type
                .map(|t| -> Element<'_, DocumentDetailsMessage> { text(t.to_string()).into() })
        };

        // Authors: free-text inputs in edit mode; hidden in view mode (shown in hero section).
        let authors_control: Option<Element<'_, DocumentDetailsMessage>> = if editing {
            let draft_authors = self.document_meta_draft.authors.as_deref().unwrap_or(&[]);
            let mut col = Column::new().spacing(space_xs);
            for (idx, author) in draft_authors.iter().enumerate() {
                col = col.push(
                    Row::new()
                        .spacing(space_xs)
                        .align_y(Vertical::Center)
                        .push(
                            widget::text_input("", author.as_str())
                                .on_input(move |v| {
                                    DocumentDetailsMessage::DocumentMetaAuthorChanged(idx, v)
                                })
                                .width(Length::Fill),
                        )
                        .push(
                            widget::button::icon(
                                widget::icon::from_name("list-remove-symbolic").size(ICON_SIZE),
                            )
                            .on_press(DocumentDetailsMessage::DocumentMetaAuthorRemoved(idx)),
                        ),
                );
            }
            col = col.push(
                widget::button::standard(fl!("document-details-document-meta-authors-add"))
                    .on_press(DocumentDetailsMessage::DocumentMetaAuthorAdded),
            );
            Some(col.into())
        } else {
            None
        };

        // Conditionally add each field row.
        //
        // `item::builder(..).control(..)` gives the label and the control equal
        // `Length::Fill` shares of the row, which is fine when the control is a
        // short, naturally-sized widget (a pick_list or a value label in view
        // mode) but starves text inputs down to half the row width. In edit
        // mode, where every control is a full-width text_input/text_editor, we
        // stack a compact label above the control instead: the control then
        // owns the entire row width, and the extra vertical line per field is
        // paid for by the cover having grown so there's more room beside it.
        let mut section = section;

        macro_rules! add_row {
            ($control:expr, $label:expr, $icon:expr) => {
                if let Some(control) = $control {
                    let row = if editing {
                        Self::stacked_field($icon, $label, control)
                    } else {
                        widget::settings::item::builder($label)
                            .icon(widget::icon::from_name($icon).size(ICON_SIZE))
                            .control(control)
                            .into()
                    };
                    section = section.add(row);
                }
            };
        }

        add_row!(
            type_control,
            fl!("document-details-document-meta-type"),
            "document-properties-symbolic"
        );
        add_row!(
            editing_only_field!(meta.title, DocumentDetailsMessage::DocumentMetaTitleChanged),
            fl!("document-details-document-meta-title"),
            "text-x-generic-symbolic"
        );
        add_row!(
            editing_only_field!(
                meta.subtitle,
                DocumentDetailsMessage::DocumentMetaSubtitleChanged
            ),
            fl!("document-details-document-meta-subtitle"),
            "text-x-generic-symbolic"
        );
        add_row!(
            authors_control,
            fl!("document-details-document-meta-authors"),
            "system-users-symbolic"
        );
        if editing {
            section = section.add(Self::stacked_field(
                "accessories-text-editor-symbolic",
                fl!("document-details-document-meta-description"),
                widget::text_editor(&self.description_content)
                    .on_action(DocumentDetailsMessage::DescriptionAction)
                    .height(Length::Fixed(120.0))
                    .into(),
            ));
        }
        add_row!(
            opt_field!(
                meta.language,
                DocumentDetailsMessage::DocumentMetaLanguageChanged
            ),
            fl!("document-details-metadata-language"),
            "preferences-desktop-locale-symbolic"
        );
        add_row!(
            opt_field!(
                meta.publisher,
                DocumentDetailsMessage::DocumentMetaPublisherChanged
            ),
            fl!("document-details-metadata-publisher"),
            "x-office-address-book-symbolic"
        );
        add_row!(
            opt_field!(
                meta.identifier,
                DocumentDetailsMessage::DocumentMetaIdentifierChanged
            ),
            fl!("document-details-metadata-identifier"),
            "dialog-information-symbolic"
        );
        add_row!(
            opt_field!(meta.date, DocumentDetailsMessage::DocumentMetaDateChanged),
            fl!("document-details-metadata-date"),
            "x-office-calendar-symbolic"
        );
        add_row!(
            opt_field!(
                meta.subject,
                DocumentDetailsMessage::DocumentMetaSubjectChanged
            ),
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
            space_m,
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

        let distinct_clients: std::collections::HashSet<_> = self
            .document
            .contents
            .iter()
            .flat_map(|c| c.sources.iter().map(|s| &s.client))
            .collect();
        let has_multiple_clients = distinct_clients.len() > 1;

        let mut header_row = vec![
            text::heading(fl!("document-details-sources")).into(),
            widget::space::horizontal().into(),
        ];
        if has_multiple_clients && !self.editing_sources {
            header_row.push(
                widget::button::icon(
                    widget::icon::from_name("emblem-synchronizing-symbolic").size(ICON_SIZE),
                )
                .on_press(DocumentDetailsMessage::SyncToAllSources)
                .tooltip(fl!("document-details-sync-to-all-sources"))
                .into(),
            );
        }
        header_row.push(edit_button.into());

        let mut sources_section =
            widget::settings::section().header(widget::settings::item_row(header_row));

        let all_sources_empty = self.document.contents.iter().all(|c| c.sources.is_empty());

        if all_sources_empty {
            sources_section = sources_section.add(widget::settings::item_row(vec![
                text(fl!("document-details-no-sources")).into(),
            ]));
        } else {
            for content in &self.document.contents {
                if content.sources.is_empty() {
                    continue;
                }

                // Group header: type badge + short fingerprint + formatted size
                let fp_short: String = content.fingerprint.chars().take(8).collect();
                let size_label = if content.size <= 0 {
                    String::new()
                } else if content.size < 1024 {
                    format!("{} B", content.size)
                } else if content.size < 1_048_576 {
                    format!("{:.1} KB", content.size as f64 / 1024.0)
                } else {
                    format!("{:.1} MB", content.size as f64 / 1_048_576.0)
                };

                let mut group_header_row = Row::new()
                    .spacing(space_xs)
                    .align_y(Vertical::Center)
                    .push(
                        widget::container(text(content.type_.as_str().to_uppercase()).size(12))
                            .class(theme::Container::Card)
                            .padding([2, 6]),
                    )
                    .push(text(fp_short).size(12));

                if !size_label.is_empty() {
                    group_header_row = group_header_row
                        .push(widget::space::horizontal())
                        .push(text(size_label).size(12));
                }

                sources_section =
                    sources_section.add(widget::settings::item_row(vec![group_header_row.into()]));

                // Sort sources within this content group: local first, then remote by URL+path
                let mut sorted_sources: Vec<_> = content.sources.iter().collect();
                sorted_sources.sort_by(|a, b| match (&a.client, &b.client) {
                    (
                        crate::client::ClientSelector::Local,
                        crate::client::ClientSelector::Local,
                    ) => a.path.cmp(&b.path),
                    (crate::client::ClientSelector::Local, _) => std::cmp::Ordering::Less,
                    (_, crate::client::ClientSelector::Local) => std::cmp::Ordering::Greater,
                    (
                        crate::client::ClientSelector::Remote(url_a),
                        crate::client::ClientSelector::Remote(url_b),
                    ) => url_a.as_str().cmp(url_b.as_str()).then(a.path.cmp(&b.path)),
                });

                for source in &sorted_sources {
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
                        .push(widget::Space::new().width(space_m))
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
                                .apply_maybe(
                                    format_imported_at(&source.imported_at),
                                    |col, date| {
                                        col.push(
                                            text(fl!("document-details-source-added", date = date))
                                                .size(12),
                                        )
                                    },
                                )
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

                    if self.editing_sources {
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

                    sources_section =
                        sources_section.add(widget::settings::item_row(vec![source_row.into()]));
                }
            }
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
                        self.document.contents.first().map(|c| c.status),
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

        let mut sections: Vec<Element<'_, DocumentDetailsMessage>> = Vec::new();
        if self.editing_document_meta {
            // Keep the cover visible next to the edit form instead of hiding the
            // whole hero while editing.
            let form = self.document_meta_section_view();
            // Larger cover while editing: the stacked label/input fields no longer
            // need half the row, so there's width to spare.
            sections.push(self.hero_row(self.selected_cover(), (300.0, 450.0), form));
        } else {
            if let Some(hero) = self.hero_section() {
                sections.push(hero);
            }
            sections.push(self.document_meta_section_view());
        }
        sections.push(status_section.into());
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
        if let Some(source) = &self.pending_source_deletion {
            return Some(crate::component::confirm_dialog::confirm_delete_dialog(
                fl!("document-details-delete-source-confirm-title"),
                fl!("document-details-delete-source-confirm-body"),
                &source.path,
                fl!("document-details-delete-source-confirm-delete"),
                fl!("document-details-delete-source-confirm-cancel"),
                DocumentDetailsMessage::ConfirmDeleteSource,
                DocumentDetailsMessage::CancelDeleteSource,
            ));
        }

        if self.show_open_picker {
            let local_sources: Vec<_> = self
                .document
                .contents
                .iter()
                .flat_map(|c| c.sources.iter().map(move |s| (c, s)))
                .filter(|(_, s)| matches!(s.client, ClientSelector::Local))
                .collect();

            // Build per-content cover map for the picker so each format shows its own thumbnail.
            let picker_covers: std::collections::HashMap<String, cosmic::widget::image::Handle> =
                local_sources
                    .iter()
                    .filter_map(|(c, _)| {
                        self.covers
                            .get(&c.fingerprint)
                            .map(|(h, _)| (c.fingerprint.clone(), h.clone()))
                    })
                    .collect();

            return Some(crate::component::source_picker::source_picker_dialog(
                fl!("document-details-open-file"),
                None,
                local_sources,
                picker_covers,
                DocumentDetailsMessage::PickOpenSource,
                DocumentDetailsMessage::CancelOpenPicker,
            ));
        }

        None
    }

    fn view_header_center(&self) -> Vec<Element<'_, DocumentDetailsMessage>> {
        let first_path = self
            .document
            .contents
            .first()
            .and_then(|c| c.sources.first())
            .map(|s| s.path.as_str())
            .unwrap_or("Unknown");
        let header_title = self
            .document
            .document_meta
            .title
            .as_deref()
            .unwrap_or_else(|| {
                Path::new(first_path)
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
                    self.document.document_guid.clone(),
                )))
                .tooltip(fl!("document-details-close"))
                .into(),
        ]
    }

    fn view_header_end(&self) -> Vec<Element<'_, DocumentDetailsMessage>> {
        let has_local = self
            .document
            .contents
            .iter()
            .flat_map(|c| c.sources.iter())
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

    fn view_context(&self) -> ContextView<'_, DocumentDetailsMessage> {
        let content: Element<'_, DocumentDetailsMessage> = if self.covers.is_empty() {
            widget::text(fl!("document-details-no-covers")).into()
        } else {
            let selected_fp = self
                .document
                .document_meta
                .selected_cover_fingerprint
                .as_deref();
            let cover_buttons: Vec<Element<'_, DocumentDetailsMessage>> = self
                .document
                .contents
                .iter()
                .filter_map(|content| {
                    let (handle, _) = self.covers.get(&content.fingerprint)?;
                    let is_selected = selected_fp == Some(content.fingerprint.as_str())
                        || (selected_fp.is_none()
                            && self
                                .document
                                .contents
                                .first()
                                .is_some_and(|c| c.fingerprint == content.fingerprint));
                    let img = widget::image(handle.clone())
                        .width(Length::Fixed(80.0))
                        .height(Length::Fixed(120.0))
                        .content_fit(ContentFit::Contain);
                    let fp = content.fingerprint.clone();
                    let type_label = widget::text(content.type_.as_str()).size(11);
                    let mut btn = widget::button::custom(
                        widget::column::with_children(vec![img.into(), type_label.into()])
                            .align_x(Horizontal::Center),
                    )
                    .width(Length::Fixed(88.0));
                    if is_selected {
                        btn = btn.class(cosmic::widget::button::ButtonClass::Suggested);
                    } else {
                        btn = btn.on_press(DocumentDetailsMessage::SelectCover(fp));
                    }
                    Some(btn.into())
                })
                .collect();
            let cover_row = widget::Row::with_children(cover_buttons).spacing(8);
            widget::container(cover_row)
                .center_x(Length::Fill)
                .padding(8)
                .into()
        };

        ContextView {
            title: fl!("document-details-select-cover"),
            content,
        }
    }

    fn update(&mut self, message: DocumentDetailsMessage) -> Task<Action<DocumentDetailsMessage>> {
        tracing::debug!("received: {message:?}");
        match message {
            DocumentDetailsMessage::CoversLoaded(map) => {
                self.covers = map;
                Task::none()
            }
            DocumentDetailsMessage::OpenCover(fingerprint) => {
                if let Some((handle, bytes)) = self.covers.get(&fingerprint) {
                    let (natural_width, natural_height) =
                        image::ImageReader::new(Cursor::new(bytes.as_slice()))
                            .with_guessed_format()
                            .ok()
                            .and_then(|r| r.into_dimensions().ok())
                            .unwrap_or((0, 0));
                    let viewer_image = ViewerImage::Raster {
                        handle: handle.clone(),
                        natural_width,
                        natural_height,
                    };
                    return task::message(DocumentDetailsMessage::Out(
                        DocumentDetailsOutput::OpenImageViewer(viewer_image),
                    ));
                }
                Task::none()
            }
            DocumentDetailsMessage::SelectCover(fingerprint) => {
                self.document.document_meta.selected_cover_fingerprint = Some(fingerprint.clone());
                self.document_meta_draft.selected_cover_fingerprint = Some(fingerprint);
                let draft = self.document_meta_draft.clone();
                let document = self.document.clone();
                let document_provider = self.document_provider.clone();
                task::future(async move {
                    let result = document_provider
                        .update_document_metadata(&document, draft)
                        .await
                        .map_err(|e| format!("{e}"));
                    DocumentDetailsMessage::CoverSelected(result)
                })
            }
            DocumentDetailsMessage::CoverSelected(result) => {
                if let Err(e) = result {
                    tracing::warn!("failed to save cover selection: {e}");
                }
                Task::none()
            }
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
                let fingerprints: Vec<String> = self
                    .document
                    .contents
                    .iter()
                    .map(|c| c.fingerprint.clone())
                    .collect();
                let document_provider = self.document_provider.clone();

                task::future(async move {
                    let mut last_err = None;
                    for fp in fingerprints {
                        if let Err(e) = document_provider.update_reading_status(&fp, status).await {
                            last_err = Some(format!("{e}"));
                        }
                    }
                    DocumentDetailsMessage::ReadingStatusUpdated(last_err.map_or(Ok(()), Err))
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
                    .contents
                    .iter()
                    .flat_map(|c| c.sources.iter())
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
                let document_guid = self.document.document_guid.clone();
                let document_provider = self.document_provider.clone();

                task::future(async move {
                    let result = document_provider
                        .refresh_single_document(&document_guid)
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
                    let new_tags: Vec<String> = document
                        .contents
                        .iter()
                        .flat_map(|c| c.tags.iter().cloned())
                        .collect::<std::collections::HashSet<_>>()
                        .into_iter()
                        .collect();
                    let set_tags_task = self
                        .tag_editor
                        .update(TagEditorMessage::SetTags(new_tags))
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
            DocumentDetailsMessage::SyncToAllSources => {
                let document = self.document.clone();
                let document_provider = self.document_provider.clone();
                task::future(async move {
                    let result = document_provider
                        .update_document(document)
                        .await
                        .map_err(|err| format!("{err}"));
                    DocumentDetailsMessage::SyncedToAllSources(result)
                })
            }
            DocumentDetailsMessage::SyncedToAllSources(result) => match result {
                Ok(()) => task::message(DocumentDetailsMessage::RefreshDocument),
                Err(err) => {
                    tracing::error!("Failed to sync document to all sources: {err}");
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
                let content = self
                    .document
                    .contents
                    .iter()
                    .find(|c| c.sources.iter().any(|s| s.guid == source.guid))
                    .cloned();
                if let Some(content) = content {
                    let document_provider = self.document_provider.clone();
                    task::future(async move {
                        let result = document_provider
                            .delete_document_source(source, content)
                            .await
                            .map_err(|err| format!("{err}"));
                        DocumentDetailsMessage::SourceDeleted(result)
                    })
                } else {
                    Task::none()
                }
            }
            DocumentDetailsMessage::SourceDeleted(result) => match result {
                Ok(()) => task::message(DocumentDetailsMessage::RefreshDocument),
                Err(err) => {
                    tracing::error!("Failed to delete source: {err}");
                    Task::none()
                }
            },
            DocumentDetailsMessage::EditDocumentMeta => {
                self.document_meta_draft = self.document.document_meta.clone();
                self.description_content = text_editor::Content::with_text(
                    self.document_meta_draft
                        .description
                        .as_deref()
                        .unwrap_or(""),
                );
                self.editing_document_meta = true;
                Task::none()
            }
            DocumentDetailsMessage::CancelDocumentMeta => {
                self.editing_document_meta = false;
                Task::none()
            }
            DocumentDetailsMessage::SaveDocumentMeta => {
                // Drop empty author entries before saving.
                if let Some(authors) = &mut self.document_meta_draft.authors {
                    authors.retain(|a| !a.trim().is_empty());
                    if authors.is_empty() {
                        self.document_meta_draft.authors = None;
                    }
                }
                let draft = self.document_meta_draft.clone();
                let document = self.document.clone();
                let document_provider = self.document_provider.clone();
                self.editing_document_meta = false;
                task::future(async move {
                    let result = document_provider
                        .update_document_metadata(&document, draft)
                        .await
                        .map_err(|e| format!("{e}"));
                    DocumentDetailsMessage::DocumentMetaSaved(result)
                })
            }
            DocumentDetailsMessage::DocumentMetaSaved(result) => match result {
                Ok(()) => task::message(DocumentDetailsMessage::RefreshDocument),
                Err(err) => {
                    tracing::error!("Failed to save document metadata: {err}");
                    Task::none()
                }
            },
            DocumentDetailsMessage::DocumentMetaTitleChanged(val) => {
                self.document_meta_draft.title = if val.is_empty() { None } else { Some(val) };
                Task::none()
            }
            DocumentDetailsMessage::DocumentMetaSubtitleChanged(val) => {
                self.document_meta_draft.subtitle = if val.is_empty() { None } else { Some(val) };
                Task::none()
            }
            DocumentDetailsMessage::DocumentMetaDocTypeChanged(val) => {
                self.document_meta_draft.document_type = val;
                Task::none()
            }
            DocumentDetailsMessage::DescriptionAction(action) => {
                self.description_content.perform(action);
                let text = self.description_content.text();
                let trimmed = text.trim_end_matches('\n');
                self.document_meta_draft.description = if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                };
                Task::none()
            }
            DocumentDetailsMessage::DocumentMetaAuthorChanged(idx, val) => {
                if let Some(authors) = &mut self.document_meta_draft.authors
                    && let Some(author) = authors.get_mut(idx)
                {
                    *author = val;
                }
                Task::none()
            }
            DocumentDetailsMessage::DocumentMetaAuthorRemoved(idx) => {
                if let Some(authors) = &mut self.document_meta_draft.authors {
                    if idx < authors.len() {
                        authors.remove(idx);
                    }
                    if authors.is_empty() {
                        self.document_meta_draft.authors = None;
                    }
                }
                Task::none()
            }
            DocumentDetailsMessage::DocumentMetaAuthorAdded => {
                self.document_meta_draft
                    .authors
                    .get_or_insert_with(Vec::new)
                    .push(String::new());
                Task::none()
            }
            DocumentDetailsMessage::DocumentMetaLanguageChanged(val) => {
                self.document_meta_draft.language = if val.is_empty() { None } else { Some(val) };
                Task::none()
            }
            DocumentDetailsMessage::DocumentMetaPublisherChanged(val) => {
                self.document_meta_draft.publisher = if val.is_empty() { None } else { Some(val) };
                Task::none()
            }
            DocumentDetailsMessage::DocumentMetaIdentifierChanged(val) => {
                self.document_meta_draft.identifier = if val.is_empty() { None } else { Some(val) };
                Task::none()
            }
            DocumentDetailsMessage::DocumentMetaDateChanged(val) => {
                self.document_meta_draft.date = if val.is_empty() { None } else { Some(val) };
                Task::none()
            }
            DocumentDetailsMessage::DocumentMetaSubjectChanged(val) => {
                self.document_meta_draft.subject = if val.is_empty() { None } else { Some(val) };
                Task::none()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use assert4rs::Assert;

    use super::*;

    #[test]
    fn format_imported_at_formats_valid_rfc3339() {
        Assert::that(format_imported_at("2026-07-15T10:30:00Z"))
            .is_some("Jul 15, 2026".to_string());
    }

    #[test]
    fn format_imported_at_returns_none_for_empty_string() {
        Assert::that(format_imported_at("")).is(None);
    }

    #[test]
    fn format_imported_at_returns_none_for_unparseable_string() {
        Assert::that(format_imported_at("not-a-date")).is(None);
    }
}
