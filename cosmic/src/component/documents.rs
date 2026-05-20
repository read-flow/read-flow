// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashSet;
use std::path::Path;

use cosmic::Action;
use cosmic::Apply;
use cosmic::Element;
use cosmic::Task;
use cosmic::cosmic_theme;
use cosmic::iced::Length;
use cosmic::iced::alignment::Vertical;
use cosmic::task;
use cosmic::theme;
use cosmic::widget;
use cosmic::widget::Column;
use cosmic::widget::Row;
use cosmic::widget::button::ButtonClass;
use provider::r#async::Provider;

use crate::ICON_SIZE;
use crate::aggregator::Document;
use crate::component::pagination::Pagination;
use crate::component::pagination::PaginationMessage;
use crate::component::provided_state::ProvidedStateMessage;
use crate::component::tag_editor::Orientation;
use crate::component::tag_editor::TagEditor;
use crate::component::tag_editor::TagEditorMessage;
use crate::component::tag_editor::TagEditorOutput;
use crate::cosmic_ext::ActionExt;
use crate::fl;
use crate::layout::layout;
use crate::state::LoadedState;
use crate::state::filtered::Filtered;

pub type DocumentState = LoadedState<Filtered<Document>>;

impl Provider<Vec<String>> for DocumentState {
    type Error = String;
    async fn provide(&self) -> Result<Vec<String>, Self::Error> {
        match self {
            LoadedState::Loaded(documents) => {
                let tags = documents
                    .unfiltered()
                    .iter()
                    .flat_map(|doc| doc.metadata.tags.clone())
                    .collect::<HashSet<_>>();

                Ok(tags.into_iter().collect::<Vec<_>>())
            }
            LoadedState::Failed(error) => Err(error.to_string()),
            _ => Err(format!("{self:?}")),
        }
    }
}

#[derive(Debug, Clone)]
pub enum DocumentsOutput {
    OpenDocumentDetails(Document),
    BatchTagEditor(TagEditorOutput),
    OpenDocument(Document),
    PickFormat(Document),
    SelectionChanged,
    NavigateToSettings,
    Scan,
}

#[derive(Debug, Clone)]
pub enum DocumentsMessage {
    Pagination(PaginationMessage),
    ToggleDocumentSelected(Document),
    ToggleAllSelected(bool),
    BatchTagEditor(TagEditorMessage),
    FilterSelectedDocuments,
    ResetBatchTagEditor,
    Out(DocumentsOutput),
}

impl From<PaginationMessage> for DocumentsMessage {
    fn from(value: PaginationMessage) -> Self {
        DocumentsMessage::Pagination(value)
    }
}

impl From<TagEditorMessage> for DocumentsMessage {
    fn from(value: TagEditorMessage) -> Self {
        match value {
            TagEditorMessage::Out(msg) => {
                DocumentsMessage::Out(DocumentsOutput::BatchTagEditor(msg))
            }
            _ => DocumentsMessage::BatchTagEditor(value),
        }
    }
}

pub struct DocumentsComponent {
    documents: DocumentState,
    pagination: Pagination,
    selected_documents: HashSet<String>,
    batch_tag_editor: TagEditor<DocumentState>, // Tag editor for batch operations
}

impl DocumentsComponent {
    pub fn new() -> (Self, Task<Action<DocumentsMessage>>) {
        let documents: DocumentState = Default::default();

        let (batch_tag_editor, init_batch_tag_editor) = TagEditor::new(
            documents.clone(),
            vec![],
            Orientation::Horizontal,
            fl!("tag-editor-select-tag"),
            fl!("tag-editor-no-tags"),
            fl!("tag-editor-remove-tag"),
        );

        (
            Self {
                documents,
                pagination: Default::default(),
                selected_documents: Default::default(),
                batch_tag_editor,
            },
            init_batch_tag_editor.map(ActionExt::map_into),
        )
    }

    pub fn view(&self) -> Element<'_, DocumentsMessage> {
        match &self.documents {
            DocumentState::New | DocumentState::Loading => {
                let cosmic_theme::Spacing { space_s, .. } = theme::active().cosmic().spacing;
                Row::new()
                    .spacing(space_s)
                    .align_y(Vertical::Center)
                    .push(
                        widget::icon::from_name("content-loading-symbolic")
                            .size(ICON_SIZE)
                            .icon(),
                    )
                    .push(widget::text(fl!("document-list-loading")))
                    .into()
            }
            DocumentState::Failed(error) => {
                widget::text(fl!("generic-error", error = error.as_str())).into()
            }
            DocumentState::Loaded(files) => self.view_files(files),
        }
    }

    fn view_files<'a>(&'a self, files: &'a Filtered<Document>) -> Element<'a, DocumentsMessage> {
        let filtered_files = files.filtered_items();
        let visible_files: Vec<_> = self
            .pagination
            .filter_visible(filtered_files.as_slice())
            .collect();

        // Handle empty state: no documents at all (onboarding) vs. filters hiding everything
        if files.unfiltered().is_empty() {
            return self.view_empty_intro();
        }

        if visible_files.is_empty() {
            return Column::new()
                .push(
                    widget::container(widget::text(fl!("document-list-no-files")))
                        .width(Length::Fill)
                        .center_x(Length::Fill)
                        .padding(32),
                )
                .into();
        }

        // Build the settings section with files
        let files_section = widget::settings::section();

        // Add select all checkbox
        let filtered_count = filtered_files.len();
        let selected_count = self.selected_documents.len();
        let all_selected = selected_count > 0 && selected_count >= filtered_count;

        let tag_editor_view = if selected_count > 0 {
            self.batch_tag_editor
                .view()
                .map(Into::into)
                .apply(widget::container)
                .width(Length::FillPortion(4))
                .into()
        } else {
            None
        };

        let checkbox_label = if all_selected {
            fl!("document-list-deselect-all")
        } else {
            fl!("document-list-select-all")
        };
        let select_all_row =
            widget::settings::item_row(vec![])
                .push(
                    widget::checkbox(all_selected)
                        .label(checkbox_label)
                        .on_toggle(DocumentsMessage::ToggleAllSelected)
                        .width(Length::FillPortion(1)),
                )
                .push(
                    widget::text(fl!(
                        "document-list-selection-count",
                        selected = selected_count,
                        total = filtered_count
                    ))
                    .width(Length::FillPortion(
                        if tag_editor_view.is_some() { 1 } else { 5 },
                    )),
                )
                .push_maybe(tag_editor_view);

        let files_section = files_section
            .add(select_all_row)
            .add(self.pagination.view().map(Into::into));

        let files_section = visible_files
            .into_iter()
            .fold(files_section, |section, file| {
                let is_selected = self.selected_documents.contains(&file.metadata.fingerprint);
                section.add(view_document(file, is_selected))
            })
            .add(self.pagination.view().map(Into::into));

        let file_content =
            widget::settings::view_column(layout(files_section).apply(|row| vec![row]));

        Column::new().push(file_content).into()
    }

    fn view_empty_intro(&self) -> Element<'_, DocumentsMessage> {
        let cosmic_theme::Spacing {
            space_m,
            space_l,
            space_xl,
            ..
        } = theme::active().cosmic().spacing;

        widget::container(
            Column::new()
                .spacing(space_l)
                .align_x(cosmic::iced::alignment::Horizontal::Center)
                .push(
                    widget::icon::from_svg_bytes(crate::app::APP_ICON)
                        .icon()
                        .size(128),
                )
                .push(widget::text::title2(fl!("document-list-empty-title")))
                .push(
                    widget::text(fl!("document-list-empty-description"))
                        .width(Length::Fixed(480.0)),
                )
                .push(
                    Row::new()
                        .spacing(space_m)
                        .push(
                            widget::button::suggested(fl!("document-list-go-to-settings"))
                                .on_press(DocumentsMessage::Out(
                                    DocumentsOutput::NavigateToSettings,
                                )),
                        )
                        .push(
                            widget::button::standard(fl!("document-list-run-scan"))
                                .on_press(DocumentsMessage::Out(DocumentsOutput::Scan)),
                        ),
                ),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding(space_xl)
        .into()
    }

    pub fn update(&mut self, message: DocumentsMessage) -> Task<Action<DocumentsMessage>> {
        match message {
            DocumentsMessage::Pagination(message) => {
                self.pagination.update(message).map(ActionExt::map_into)
            }
            DocumentsMessage::ToggleDocumentSelected(document) => {
                let fingerprint = document.metadata.fingerprint.clone();
                if self.selected_documents.contains(&fingerprint) {
                    self.selected_documents.remove(&fingerprint);
                } else {
                    self.selected_documents.insert(fingerprint);
                }
                self.notify_selection_changed()
            }
            DocumentsMessage::ToggleAllSelected(all_selected) => {
                if all_selected {
                    if let DocumentState::Loaded(files) = &self.documents {
                        for file in files.filtered_items() {
                            self.selected_documents
                                .insert(file.metadata.fingerprint.clone());
                        }
                    }
                } else {
                    self.selected_documents.clear();
                }
                self.notify_selection_changed()
            }
            DocumentsMessage::FilterSelectedDocuments => {
                if let DocumentState::Loaded(files) = &self.documents {
                    let selected_document_count = self.selected_documents.len();
                    let filtered_fingerprints = files
                        .unfiltered()
                        .iter()
                        .map(|doc| doc.metadata.fingerprint.clone())
                        .collect::<HashSet<_>>();
                    self.selected_documents
                        .retain(|doc| filtered_fingerprints.contains(doc));
                    if self.selected_documents.len() != selected_document_count {
                        self.notify_selection_changed()
                    } else {
                        Task::none()
                    }
                } else {
                    Task::none()
                }
            }
            DocumentsMessage::BatchTagEditor(tag_editor_message) => self
                .batch_tag_editor
                .update(tag_editor_message)
                .map(ActionExt::map_into),
            DocumentsMessage::ResetBatchTagEditor => {
                let selected_documents = self.get_selected_documents();
                let common_tags = get_common_tags(&selected_documents);
                Task::batch(vec![
                    task::message(DocumentsMessage::BatchTagEditor(TagEditorMessage::SetTags(
                        common_tags,
                    ))),
                    task::message(DocumentsMessage::BatchTagEditor(TagEditorMessage::Tags(
                        ProvidedStateMessage::Load,
                    ))),
                ])
            }
            DocumentsMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent component")
            }
        }
    }

    pub fn set_filtered_indices(&mut self, files: Vec<usize>) {
        self.documents.unwrap_mut().set_filtered_indices(files);
    }

    fn notify_selection_changed(&self) -> Task<Action<DocumentsMessage>> {
        if let DocumentState::Loaded(_) = &self.documents {
            cosmic::task::message(DocumentsMessage::Out(DocumentsOutput::SelectionChanged))
                .chain(task::message(DocumentsMessage::ResetBatchTagEditor))
        } else {
            Task::none()
        }
    }

    pub fn get_selected_documents(&self) -> Vec<Document> {
        if let DocumentState::Loaded(files) = &self.documents {
            files
                .unfiltered()
                .iter()
                .filter(|doc| self.selected_documents.contains(&doc.metadata.fingerprint))
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    }

    pub fn set_document_state(&mut self, state: DocumentState) -> Task<Action<DocumentsMessage>> {
        self.documents = state.clone();
        self.batch_tag_editor.set_provider(state);
        task::message(DocumentsMessage::ResetBatchTagEditor)
    }

    pub fn update_item<F>(&mut self, search_fn: F, item: Document) -> Task<Action<DocumentsMessage>>
    where
        F: FnMut(&&mut Document) -> bool + Clone,
    {
        self.documents
            .unwrap_mut()
            .update_item(search_fn.clone(), item.clone());
        self.batch_tag_editor
            .provider_mut()
            .unwrap_mut()
            .update_item(search_fn, item);

        task::message(DocumentsMessage::BatchTagEditor(TagEditorMessage::Tags(
            ProvidedStateMessage::Load,
        )))
    }

    pub fn is_loaded(&self) -> bool {
        self.documents.is_loaded()
    }

    pub fn unfiltered(&self) -> &[Document] {
        self.documents.unwrap().unfiltered()
    }

    pub fn sort_unfiltered<F>(&mut self, sort_fn: F)
    where
        F: FnMut(&mut [Document]) + Clone,
    {
        self.documents.unwrap_mut().sort_unfiltered(sort_fn.clone());
        self.batch_tag_editor
            .provider_mut()
            .unwrap_mut()
            .sort_unfiltered(sort_fn);
    }
}

fn view_document<'a>(document: &'a Document, is_selected: bool) -> Element<'a, DocumentsMessage> {
    let file_types = document.file_types();
    let icon_name = document.metadata.type_.get_file_type_icon();

    let (selected_icon_name, selected_icon_class) = if is_selected {
        ("checkbox-checked-symbolic", ButtonClass::Suggested)
    } else {
        ("checkbox-symbolic", ButtonClass::Icon)
    };

    let open_msg = if file_types.len() > 1 {
        DocumentsMessage::Out(DocumentsOutput::PickFormat(document.clone()))
    } else {
        DocumentsMessage::Out(DocumentsOutput::OpenDocument(document.clone()))
    };

    vec![
        widget::button::icon(widget::icon::from_name(selected_icon_name).size(ICON_SIZE))
            .class(selected_icon_class)
            .on_press(DocumentsMessage::ToggleDocumentSelected(document.clone()))
            .into(),
        widget::icon::from_name(icon_name)
            .size(ICON_SIZE)
            .icon()
            .into(),
        display_document_title(document),
        display_pills(document),
        widget::button::icon(
            widget::icon::from_name("dialog-information-symbolic").size(ICON_SIZE),
        )
        .on_press(DocumentsMessage::Out(DocumentsOutput::OpenDocumentDetails(
            document.clone(),
        )))
        .tooltip(fl!("document-list-open-document-details"))
        .into(),
    ]
    .apply(widget::settings::item_row)
    .apply(widget::button::custom)
    .width(Length::Fill)
    .class(ButtonClass::ListItem)
    .on_press(open_msg)
    .into()
}

fn display_document_title<'a>(document: &'a Document) -> Element<'a, DocumentsMessage> {
    let cosmic_theme::Spacing { space_xxs, .. } = theme::active().cosmic().spacing;

    let path: &Path = document.local_or_any_source().path.as_ref();

    let primary = document
        .user_meta
        .title
        .as_deref()
        .unwrap_or_else(|| path.file_name().and_then(|n| n.to_str()).unwrap_or(""));

    let secondary: String = if let Some(authors) = document.user_meta.authors.as_deref() {
        if !authors.is_empty() {
            authors.join(", ")
        } else {
            path.parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        }
    } else {
        path.parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default()
    };

    cosmic::iced::widget::column![
        widget::text(primary.to_string()),
        widget::text(secondary).size(11),
    ]
    .spacing(space_xxs)
    .apply(widget::container)
    .width(Length::Fill)
    .into()
}

fn pill<'a, Message: 'a>(label: impl ToString) -> Element<'a, Message> {
    let cosmic_theme::Spacing {
        space_xxs,
        space_xs,
        ..
    } = theme::active().cosmic().spacing;
    widget::text::caption(label.to_string())
        .apply(widget::container)
        .class(cosmic::theme::Container::Card)
        .padding([space_xxs, space_xs])
        .into()
}

fn display_pills<'a>(document: &'a Document) -> Element<'a, DocumentsMessage> {
    let cosmic_theme::Spacing { space_xxs, .. } = theme::active().cosmic().spacing;
    const MAX_TAGS: usize = 3;

    let mut row = Row::new().spacing(space_xxs).align_y(Vertical::Center);

    // File type pills when multiple formats exist
    let file_types = document.file_types();
    if file_types.len() > 1 {
        for t in &file_types {
            row = row.push(pill(t.as_str().to_uppercase()));
        }
    }

    // Document type pill (if set)
    if let Some(doc_type) = &document.user_meta.document_type {
        row = row.push(pill(doc_type.to_string()));
    }

    // Tag pills (up to MAX_TAGS, then a count badge)
    let tags = &document.metadata.tags;
    let shown = tags.len().min(MAX_TAGS);
    for tag in &tags[..shown] {
        row = row.push(pill(tag));
    }
    if tags.len() > MAX_TAGS {
        row = row.push(pill(format!("+{}", tags.len() - MAX_TAGS)));
    }

    row.into()
}

/// Get tags that are common to all selected documents
pub fn get_common_tags(selected_documents: &[Document]) -> Vec<String> {
    let common_tags = selected_documents
        .iter()
        .map(|document| {
            document
                .metadata
                .tags
                .clone()
                .into_iter()
                .collect::<HashSet<String>>()
        })
        .reduce(|acc, document_tags| acc.intersection(&document_tags).cloned().collect())
        .unwrap_or_else(HashSet::new);

    let mut common_tags = common_tags.into_iter().collect::<Vec<_>>();

    common_tags.sort();

    common_tags
}
