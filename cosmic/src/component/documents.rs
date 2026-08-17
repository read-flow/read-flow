// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;

use cosmic::Action;
use cosmic::Apply;
use cosmic::Element;
use cosmic::Task;
use cosmic::cosmic_theme;
use cosmic::iced::ContentFit;
use cosmic::iced::Length;
use cosmic::iced::alignment::Vertical;
use cosmic::task;
use cosmic::theme;
use cosmic::widget;
use cosmic::widget::Column;
use cosmic::widget::Row;
use cosmic::widget::button::ButtonClass;
use provider::r#async::Value;

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

/// Unique tags across every document in the (unfiltered) collection.
fn compute_all_tags(documents: &Filtered<Document>) -> Vec<String> {
    documents
        .unfiltered()
        .iter()
        .flat_map(|doc| doc.contents.iter().flat_map(|c| c.tags.iter().cloned()))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect()
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
    MergeDocuments,
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

/// @feature: documents.batch_tag
pub struct DocumentsComponent {
    documents: DocumentState,
    pagination: Pagination,
    selected_documents: HashSet<String>,
    batch_tag_editor: TagEditor<Value<Vec<String>>>, // Tag editor for batch operations
    /// Tags present on some, but not all, selected documents, with how many
    /// selected documents have each one. Recomputed alongside the batch tag
    /// editor's common-tags list.
    partial_tags: Vec<(String, usize)>,
    covers: HashMap<String, widget::image::Handle>,
}

impl DocumentsComponent {
    pub fn new() -> (Self, Task<Action<DocumentsMessage>>) {
        let documents: DocumentState = Default::default();

        let (batch_tag_editor, init_batch_tag_editor) = TagEditor::new(
            Value::new(Vec::new()),
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
                partial_tags: Vec::new(),
                covers: HashMap::new(),
            },
            init_batch_tag_editor.map(ActionExt::map_into),
        )
    }

    pub fn set_covers(&mut self, covers: HashMap<String, widget::image::Handle>) {
        self.covers = covers;
    }

    pub fn covers(&self) -> &HashMap<String, widget::image::Handle> {
        &self.covers
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

        let tag_editor_row: Option<Element<'_, DocumentsMessage>> = if selected_count > 0 {
            let cosmic_theme::Spacing { space_xxs, .. } = theme::active().cosmic().spacing;
            Some(
                Column::new()
                    .spacing(space_xxs)
                    .push(widget::text::caption(fl!(
                        "document-list-common-tags-label"
                    )))
                    .push(self.batch_tag_editor.view().map(Into::into))
                    .width(Length::Fill)
                    .into(),
            )
        } else {
            None
        };

        let partial_tags_row: Option<Element<'_, DocumentsMessage>> =
            if self.partial_tags.is_empty() {
                None
            } else {
                let cosmic_theme::Spacing { space_xxs, .. } = theme::active().cosmic().spacing;
                let chips = self
                    .partial_tags
                    .iter()
                    .map(|(tag, count)| {
                        widget::button::text(format!("{tag} ({count}/{selected_count})"))
                            .trailing_icon(widget::icon::from_name("edit-delete-symbolic"))
                            .on_press(DocumentsMessage::Out(DocumentsOutput::BatchTagEditor(
                                TagEditorOutput::TagRemoved(tag.clone()),
                            )))
                            .tooltip(fl!("document-list-partial-tag-remove-tooltip"))
                            .into()
                    })
                    .collect::<Vec<_>>();
                Some(
                    Column::new()
                        .spacing(space_xxs)
                        .push(widget::text::caption(fl!(
                            "document-list-partial-tags-label"
                        )))
                        .push(
                            chips
                                .apply(widget::flex_row)
                                .spacing(space_xxs)
                                .width(Length::Fill),
                        )
                        .width(Length::Fill)
                        .into(),
                )
            };

        let merge_button: Option<Element<'_, DocumentsMessage>> = if selected_count >= 2 {
            Some(
                widget::button::suggested(fl!("document-list-merge"))
                    .on_press(DocumentsMessage::Out(DocumentsOutput::MergeDocuments))
                    .into(),
            )
        } else {
            None
        };

        let checkbox_label = if all_selected {
            fl!("document-list-deselect-all")
        } else {
            fl!("document-list-select-all")
        };
        let count_fill = if merge_button.is_some() { 4 } else { 5 };
        let select_all_row = widget::settings::item_row(vec![])
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
                .width(Length::FillPortion(count_fill)),
            )
            .push_maybe(merge_button);

        let files_section = files_section
            .add(select_all_row)
            .add_maybe(tag_editor_row)
            .add_maybe(partial_tags_row)
            .add(self.pagination.view().map(Into::into));

        let files_section = visible_files
            .into_iter()
            .fold(files_section, |section, file| {
                let is_selected = self.selected_documents.contains(&file.document_guid);
                let cover = file
                    .document_meta
                    .selected_cover_fingerprint
                    .as_ref()
                    .and_then(|fp| self.covers.get(fp))
                    .or_else(|| {
                        file.contents
                            .first()
                            .and_then(|c| self.covers.get(&c.fingerprint))
                    });
                section.add(view_document(file, is_selected, selected_count > 0, cover))
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
                let guid = document.document_guid.clone();
                if self.selected_documents.contains(&guid) {
                    self.selected_documents.remove(&guid);
                } else {
                    self.selected_documents.insert(guid);
                }
                self.notify_selection_changed()
            }
            DocumentsMessage::ToggleAllSelected(all_selected) => {
                if all_selected {
                    if let DocumentState::Loaded(files) = &self.documents {
                        for file in files.filtered_items() {
                            self.selected_documents.insert(file.document_guid.clone());
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
                    let filtered_guids = files
                        .unfiltered()
                        .iter()
                        .map(|doc| doc.document_guid.clone())
                        .collect::<HashSet<_>>();
                    self.selected_documents
                        .retain(|doc| filtered_guids.contains(doc));
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
                self.partial_tags = get_partial_tags(&selected_documents);
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
        if let Some(docs) = self.documents.get_loaded_mut() {
            docs.set_filtered_indices(files);
        }
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
                .filter(|doc| self.selected_documents.contains(&doc.document_guid))
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    }

    pub fn set_document_state(&mut self, state: DocumentState) -> Task<Action<DocumentsMessage>> {
        let all_tags = state.get_loaded().map(compute_all_tags).unwrap_or_default();
        self.batch_tag_editor.set_provider(Value::new(all_tags));
        self.documents = state;
        task::message(DocumentsMessage::ResetBatchTagEditor)
    }

    pub fn update_item<F>(&mut self, search_fn: F, item: Document) -> Task<Action<DocumentsMessage>>
    where
        F: FnMut(&&mut Document) -> bool,
    {
        let Some(docs) = self.documents.get_loaded_mut() else {
            return Task::none();
        };
        docs.update_item(search_fn, item.clone());

        // Merge the edited document's tags into the cached tag list instead of
        // refetching (which would rescan every document in the library).
        self.batch_tag_editor.merge_tags(document_tags(&item));
        Task::none()
    }

    pub fn is_loaded(&self) -> bool {
        self.documents.is_loaded()
    }

    pub fn unfiltered(&self) -> &[Document] {
        self.documents
            .get_loaded()
            .map(|d| d.unfiltered())
            .unwrap_or(&[])
    }

    pub fn sort_unfiltered<F>(&mut self, sort_fn: F)
    where
        F: FnMut(&mut [Document]),
    {
        if let Some(docs) = self.documents.get_loaded_mut() {
            docs.sort_unfiltered(sort_fn);
        }
    }
}

fn view_document<'a>(
    document: &'a Document,
    is_selected: bool,
    selection_active: bool,
    cover: Option<&'a widget::image::Handle>,
) -> Element<'a, DocumentsMessage> {
    let total_sources: usize = document.contents.iter().map(|c| c.sources.len()).sum();
    let row_press_msg = if selection_active {
        DocumentsMessage::ToggleDocumentSelected(document.clone())
    } else if total_sources > 1 {
        DocumentsMessage::Out(DocumentsOutput::PickFormat(document.clone()))
    } else {
        DocumentsMessage::Out(DocumentsOutput::OpenDocument(document.clone()))
    };

    let cover_widget: Element<'a, DocumentsMessage> = match cover {
        Some(handle) => widget::image(handle.clone())
            .width(Length::Fixed(32.0))
            .height(Length::Fixed(48.0))
            .content_fit(ContentFit::Contain)
            .into(),
        None => widget::Space::new()
            .width(Length::Fixed(32.0))
            .height(Length::Fixed(48.0))
            .into(),
    };

    vec![
        widget::checkbox(is_selected)
            .on_toggle({
                let document = document.clone();
                move |_checked| DocumentsMessage::ToggleDocumentSelected(document.clone())
            })
            .into(),
        cover_widget,
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
    .class(ButtonClass::ListItem(theme::active().cosmic().radius_s()))
    .on_press(row_press_msg)
    .into()
}

fn display_document_title<'a>(document: &'a Document) -> Element<'a, DocumentsMessage> {
    let cosmic_theme::Spacing { space_xxs, .. } = theme::active().cosmic().spacing;

    let path: Option<&Path> = document.local_or_any_source().map(|(_, s)| s.path.as_ref());

    let primary = document
        .document_meta
        .title
        .as_deref()
        .unwrap_or_else(|| path.and_then(|p| p.file_name()?.to_str()).unwrap_or(""));

    let secondary: String = if let Some(authors) = document.document_meta.authors.as_deref() {
        if !authors.is_empty() {
            authors.join(", ")
        } else {
            path.and_then(|p| p.parent())
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        }
    } else {
        path.and_then(|p| p.parent())
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

pub(crate) fn pill<'a, Message: 'a>(label: impl ToString) -> Element<'a, Message> {
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
    if let Some(doc_type) = &document.document_meta.document_type {
        row = row.push(pill(doc_type.to_string()));
    }

    // Tag pills (up to MAX_TAGS, then a count badge)
    let all_tags = document_tags(document);
    let shown = all_tags.len().min(MAX_TAGS);
    for tag in &all_tags[..shown] {
        row = row.push(pill(tag));
    }
    if all_tags.len() > MAX_TAGS {
        row = row.push(pill(format!("+{}", all_tags.len() - MAX_TAGS)));
    }

    row.into()
}

/// Unique tags across all of a document's contents, in first-seen order.
fn document_tags(document: &Document) -> Vec<String> {
    let mut seen = HashSet::new();
    document
        .contents
        .iter()
        .flat_map(|c| c.tags.iter().cloned())
        .filter(|t| seen.insert(t.clone()))
        .collect()
}

/// Get tags that are common to all selected documents
pub fn get_common_tags(selected_documents: &[Document]) -> Vec<String> {
    let common_tags = selected_documents
        .iter()
        .map(|document| document_tags(document).into_iter().collect::<HashSet<_>>())
        .reduce(|acc, tags| acc.intersection(&tags).cloned().collect())
        .unwrap_or_else(HashSet::new);

    let mut common_tags = common_tags.into_iter().collect::<Vec<_>>();

    common_tags.sort();

    common_tags
}

/// Tags present on some, but not all, selected documents, each with how many
/// of the selected documents have it. Excludes tags common to every selected
/// document (those belong to `get_common_tags` instead).
pub fn get_partial_tags(selected_documents: &[Document]) -> Vec<(String, usize)> {
    let total = selected_documents.len();
    let mut counts: HashMap<String, usize> = HashMap::new();
    for document in selected_documents {
        for tag in document_tags(document) {
            *counts.entry(tag).or_insert(0) += 1;
        }
    }

    let mut partial_tags: Vec<(String, usize)> = counts
        .into_iter()
        .filter(|(_, count)| *count < total)
        .collect();
    partial_tags.sort();

    partial_tags
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use read_flow_core::api::ReadingStatus;

    use super::Document;
    use super::compute_all_tags;
    use super::document_tags;
    use super::get_common_tags;
    use super::get_partial_tags;
    use crate::aggregator::DocumentContent;
    use crate::aggregator::DocumentType;
    use crate::state::filtered::Filtered;

    fn content(tags: Vec<&str>) -> DocumentContent {
        DocumentContent {
            fingerprint: "fp".to_string(),
            type_: DocumentType::Epub,
            size: 0,
            tags: tags.into_iter().map(String::from).collect(),
            status: ReadingStatus::Unread,
            sources: vec![],
        }
    }

    fn doc(guid: &str, contents: Vec<DocumentContent>) -> Document {
        Document {
            document_guid: guid.to_string(),
            document_meta: Default::default(),
            contents,
        }
    }

    #[test]
    fn document_tags_dedupes_across_contents_preserving_first_seen_order() {
        let document = doc(
            "d1",
            vec![
                content(vec!["rust", "fiction"]),
                content(vec!["fiction", "programming"]),
            ],
        );

        assert_eq!(
            document_tags(&document),
            vec![
                "rust".to_string(),
                "fiction".to_string(),
                "programming".to_string()
            ]
        );
    }

    #[test]
    fn get_common_tags_intersects_across_selected_documents() {
        let selected = vec![
            doc("d1", vec![content(vec!["rust", "fiction"])]),
            doc("d2", vec![content(vec!["rust", "programming"])]),
        ];

        assert_eq!(get_common_tags(&selected), vec!["rust".to_string()]);
    }

    #[test]
    fn get_common_tags_empty_when_no_documents_selected() {
        assert_eq!(get_common_tags(&[]), Vec::<String>::new());
    }

    #[test]
    fn get_partial_tags_excludes_tags_common_to_all_and_reports_counts() {
        let selected = vec![
            doc("d1", vec![content(vec!["rust", "fiction"])]),
            doc("d2", vec![content(vec!["rust", "programming"])]),
            doc("d3", vec![content(vec!["rust"])]),
        ];

        // "rust" is on all 3 (common, not partial); "fiction" and
        // "programming" are each on exactly 1 of the 3.
        assert_eq!(
            get_partial_tags(&selected),
            vec![("fiction".to_string(), 1), ("programming".to_string(), 1),]
        );
    }

    #[test]
    fn get_partial_tags_empty_when_no_documents_selected() {
        assert_eq!(get_partial_tags(&[]), Vec::new());
    }

    #[test]
    fn compute_all_tags_dedupes_across_the_whole_collection() {
        let files = Filtered::new(vec![
            doc("d1", vec![content(vec!["rust", "fiction"])]),
            doc("d2", vec![content(vec!["rust", "programming"])]),
        ]);

        let tags: HashSet<String> = compute_all_tags(&files).into_iter().collect();
        assert_eq!(
            tags,
            HashSet::from([
                "rust".to_string(),
                "fiction".to_string(),
                "programming".to_string(),
            ])
        );
    }
}
