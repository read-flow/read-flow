// SPDX-License-Identifier: GPL-3.0-or-later

use std::cmp::Ordering;
use std::collections::HashSet;
use std::fmt;
use std::slice;
use std::sync::Arc;

use archive_organizer::api::ReadingStatus;
use cosmic::Action;
use cosmic::Apply;
use cosmic::Element;
use cosmic::Task;
use cosmic::iced;
use cosmic::iced::Length;
use cosmic::task;
use cosmic::widget;

use crate::aggregator::Document;
use crate::aggregator::Documents;
use crate::app::ContextView;
use crate::client::ClientSelector;
use crate::component::documents::DocumentState;
use crate::component::documents::DocumentsComponent;
use crate::component::documents::DocumentsMessage;
use crate::component::documents::DocumentsOutput;
use crate::component::pagination::PaginationMessage;
use crate::component::provided_state::ProvidedStateMessage;
use crate::component::tag_editor::TagEditorOutput;
use crate::component::tag_filter::TagFilter;
use crate::component::tag_filter::TagFilterMessage;
use crate::component::tag_filter::TagFilterOutput;
use crate::cosmic_ext::ActionExt;
use crate::document_provider::DocumentProvider;
use crate::fl;
use crate::page::Page;
use crate::state::filtered::Filtered;

/// Sort options for the document list
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DocumentSortOption {
    #[default]
    FilenameAsc,
    FilenameDesc,
    SizeAsc,
    SizeDesc,
    TypeAsc,
    TypeDesc,
    StatusAsc,
    StatusDesc,
}

impl DocumentSortOption {
    pub const ALL: &'static [Self] = &[
        Self::FilenameAsc,
        Self::FilenameDesc,
        Self::SizeAsc,
        Self::SizeDesc,
        Self::TypeAsc,
        Self::TypeDesc,
        Self::StatusAsc,
        Self::StatusDesc,
    ];
}

impl fmt::Display for DocumentSortOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::FilenameAsc => fl!("document-list-sort-filename-asc"),
            Self::FilenameDesc => fl!("document-list-sort-filename-desc"),
            Self::SizeAsc => fl!("document-list-sort-size-asc"),
            Self::SizeDesc => fl!("document-list-sort-size-desc"),
            Self::TypeAsc => fl!("document-list-sort-type-asc"),
            Self::TypeDesc => fl!("document-list-sort-type-desc"),
            Self::StatusAsc => fl!("document-list-sort-status-asc"),
            Self::StatusDesc => fl!("document-list-sort-status-desc"),
        };
        write!(f, "{}", label)
    }
}

pub struct DocumentList {
    pub(super) document_provider: Arc<DocumentProvider>,
    archive: DocumentsComponent,
    is_filtering: bool,                   // Track if filtering is in progress
    search_query: String,                 // The search query string
    search_input_id: widget::Id,          // Unique ID for focus management
    debounce_counter: u32,                // Counter to track debounce state
    sort_option: DocumentSortOption,      // Current sort option
    status_filter: Option<ReadingStatus>, // Optional reading status filter
    tag_filter: TagFilter<Arc<DocumentProvider>>, // Tag Filter component
    source_filter: Option<ClientSelector>, // Optional source filter
    available_sources: Vec<ClientSelector>, // Available sources for filtering
}

#[derive(Debug, Clone)]
pub enum DocumentListOutput {
    OpenDetails(Document),
}

#[derive(Debug, Clone)]
pub enum DocumentListMessage {
    LoadArchive,
    Loaded(Documents),
    LoadingFailed(String),
    RefreshDocument(Document),
    SearchChanged(String),
    ClearSearch,
    FilteringComplete(Vec<usize>),
    FocusSearchInput,
    DebounceTimeout(u32, String), // (counter, query) - triggers filtering after delay
    SortChanged(DocumentSortOption),
    StatusFilterChanged(Option<ReadingStatus>),
    ClearStatusFilter,
    SourceFilterChanged(Option<ClientSelector>),
    ClearSourceFilter,
    TagFilter(TagFilterMessage),
    DocumentsComponent(DocumentsMessage),
    SetAvailableSources(Vec<ClientSelector>),
    Out(DocumentListOutput),
}

impl From<TagFilterMessage> for DocumentListMessage {
    fn from(value: TagFilterMessage) -> Self {
        Self::TagFilter(value)
    }
}

impl From<DocumentsMessage> for DocumentListMessage {
    fn from(value: DocumentsMessage) -> Self {
        Self::DocumentsComponent(value)
    }
}

impl DocumentList {
    /// Start debounce timer - waits for user to stop typing before filtering
    fn start_debounce_timer(
        &self,
        counter: u32,
        query: String,
    ) -> Task<Action<DocumentListMessage>> {
        task::future(async move {
            // Wait 250ms for user to stop typing
            tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;
            DocumentListMessage::DebounceTimeout(counter, query)
        })
    }

    /// Start background filtering task (called after debounce timeout)
    fn start_background_filtering(
        &self,
        query: String,
        status_filter: Option<ReadingStatus>,
        source_filter: Option<ClientSelector>,
        allow_tags: HashSet<String>,
        deny_tags: HashSet<String>,
        all_files: Vec<Document>,
    ) -> Task<Action<DocumentListMessage>> {
        task::future(async move {
            // Perform filtering in background after debounce timeout
            // This runs only when user has paused typing for 250ms
            let filtered_files = all_files
                .iter()
                .enumerate()
                .filter_map(|(index, file)| {
                    filter_document(
                        &query,
                        status_filter,
                        source_filter.as_ref(),
                        &allow_tags,
                        &deny_tags,
                        &file,
                    )
                    .then_some(index)
                })
                .collect();

            DocumentListMessage::FilteringComplete(filtered_files)
        })
    }

    pub fn new(
        document_provider: Arc<DocumentProvider>,
    ) -> (Self, Task<Action<DocumentListMessage>>) {
        let (tag_filter, tag_filter_init) = TagFilter::new(document_provider.clone());

        let (archive, archive_init) = DocumentsComponent::new();

        (
            Self {
                document_provider: document_provider.clone(),
                archive,
                search_query: String::new(),
                is_filtering: false,
                search_input_id: widget::Id::unique(),
                debounce_counter: 0,
                sort_option: DocumentSortOption::default(),
                status_filter: None,
                tag_filter,
                source_filter: None,
                available_sources: Default::default(),
            },
            Task::batch(vec![
                tag_filter_init.map(ActionExt::map_into),
                archive_init.map(ActionExt::map_into),
                task::message(DocumentListMessage::LoadArchive),
                task::message(DocumentListMessage::FocusSearchInput),
                task::future(async move {
                    DocumentListMessage::SetAvailableSources(
                        document_provider.get_client_selectors().await,
                    )
                }),
            ]),
        )
    }

    fn handle_batch_tag_editor_output(
        &mut self,
        msg: TagEditorOutput,
    ) -> Task<Action<DocumentListMessage>> {
        match msg {
            TagEditorOutput::TagAdded(new_tag) => self.batch_add_tags(new_tag),
            TagEditorOutput::TagRemoved(removed_tag) => self.batch_remove_tags(removed_tag),
            TagEditorOutput::TagsUpdated(_tags) => Task::none(),
        }
    }

    fn batch_remove_tags(&mut self, removed_tag: String) -> Task<Action<DocumentListMessage>> {
        let selected_documents = self.archive.get_selected_documents();
        let document_provider = self.document_provider.clone();
        task::future(async move {
            let _ = document_provider
                .batch_delete_document_tags(selected_documents, slice::from_ref(&removed_tag))
                .await;
            DocumentListMessage::LoadArchive
        })
    }

    fn batch_add_tags(&mut self, new_tag: String) -> Task<Action<DocumentListMessage>> {
        let selected_documents = self.archive.get_selected_documents();
        let document_provider = self.document_provider.clone();
        task::future(async move {
            let _ = document_provider
                .batch_add_document_tags(selected_documents, slice::from_ref(&new_tag))
                .await;

            DocumentListMessage::LoadArchive
        })
    }

    fn filter_now(&mut self) -> Task<Action<DocumentListMessage>> {
        // Increment debounce counter to invalidate previous timers
        self.debounce_counter += 1;

        if self.archive.is_loaded() && !self.is_filtering {
            self.is_filtering = true;
            self.start_background_filtering(
                self.search_query.clone(),
                self.status_filter,
                self.source_filter.clone(),
                self.tag_filter.allow_tags.clone(),
                self.tag_filter.deny_tags.clone(),
                self.archive.unfiltered().to_vec(),
            )
        } else {
            Task::none()
        }
    }

    fn sort_now(&mut self) -> Task<Action<DocumentListMessage>> {
        if self.archive.is_loaded() {
            // Sort the unfiltered documents in place
            self.archive
                .sort_unfiltered(|docs| sort_documents(docs, self.sort_option));
            // Re-apply the current filter to update the filtered view
            self.filter_now()
        } else {
            Task::none()
        }
    }
}

/// Sort documents based on the selected sort option
fn sort_documents(documents: &mut [Document], sort_option: DocumentSortOption) {
    documents.sort_by(|a, b| compare_documents(a, b, sort_option));
}

/// Compare two documents based on the sort option
fn compare_documents(a: &Document, b: &Document, sort_option: DocumentSortOption) -> Ordering {
    match sort_option {
        DocumentSortOption::FilenameAsc => get_filename(a)
            .to_lowercase()
            .cmp(&get_filename(b).to_lowercase()),
        DocumentSortOption::FilenameDesc => get_filename(b)
            .to_lowercase()
            .cmp(&get_filename(a).to_lowercase()),
        DocumentSortOption::SizeAsc => a.metadata.size.cmp(&b.metadata.size),
        DocumentSortOption::SizeDesc => b.metadata.size.cmp(&a.metadata.size),
        DocumentSortOption::TypeAsc => a.metadata.type_.as_str().cmp(b.metadata.type_.as_str()),
        DocumentSortOption::TypeDesc => b.metadata.type_.as_str().cmp(a.metadata.type_.as_str()),
        DocumentSortOption::StatusAsc => {
            status_order(&a.metadata.status).cmp(&status_order(&b.metadata.status))
        }
        DocumentSortOption::StatusDesc => {
            status_order(&b.metadata.status).cmp(&status_order(&a.metadata.status))
        }
    }
}

/// Get the filename from a document (uses local source if available, otherwise any source)
fn get_filename(doc: &Document) -> &str {
    let source = doc.local_or_any_source();
    source.path.rsplit('/').next().unwrap_or(&source.path)
}

/// Convert reading status to a sortable order (Unread=0, Reading=1, Read=2)
fn status_order(status: &ReadingStatus) -> u8 {
    match status {
        ReadingStatus::Unread => 0,
        ReadingStatus::Reading => 1,
        ReadingStatus::Read => 2,
    }
}

fn filter_document(
    search_query: &str,
    status_filter: Option<ReadingStatus>,
    source_filter: Option<&ClientSelector>,
    allow_tags: &HashSet<String>,
    deny_tags: &HashSet<String>,
    document: &&Document,
) -> bool {
    // Filter by search query
    let matches_search = if search_query.is_empty() {
        true
    } else {
        let query = search_query.to_lowercase();
        let path_matches = document
            .sources
            .iter()
            .map(|source| source.path.to_lowercase())
            .filter(|path| path.contains(&query))
            .count();
        let tags_lower = document.metadata.tags.join(" ").to_lowercase();
        path_matches > 0 || tags_lower.contains(&query)
    };

    // Filter by reading status
    let matches_status = status_filter.is_none_or(|status| document.metadata.status == status);

    // Filter by source (document must exist on the selected source)
    let matches_source = source_filter.is_none_or(|source| {
        document
            .sources
            .iter()
            .any(|doc_source| &doc_source.client == source)
    });

    // Filter by allowed tags (file must have ALL allowed tags)
    let matches_allow_tags = allow_tags.is_empty()
        || allow_tags
            .iter()
            .all(|tag| document.metadata.tags.contains(tag));

    // Filter by denied tags (file must have NONE of the denied tags)
    let matches_deny_tags = deny_tags.is_empty()
        || !document
            .metadata
            .tags
            .iter()
            .any(|tag| deny_tags.contains(tag));

    matches_search && matches_status && matches_source && matches_allow_tags && matches_deny_tags
}

impl Page for DocumentList {
    type Message = DocumentListMessage;

    fn view(&self) -> Element<'_, DocumentListMessage> {
        self.archive
            .view()
            .map(Into::into)
            .apply(widget::scrollable::vertical)
            .into()
    }

    fn view_header_center(&self) -> Vec<Element<'_, DocumentListMessage>> {
        let mut elements: Vec<Element<'_, DocumentListMessage>> = vec![
            widget::search_input(fl!("document-list-search-placeholder"), &self.search_query)
                .id(self.search_input_id.clone())
                .always_active()
                .on_input(DocumentListMessage::SearchChanged)
                .on_clear(DocumentListMessage::ClearSearch)
                .width(Length::Fixed(300.0))
                .into(),
        ];

        if self.is_filtering {
            elements.push(widget::text(fl!("document-list-filtering")).size(12).into());
        }

        elements
    }

    fn view_header_end(&self) -> Vec<Element<'_, DocumentListMessage>> {
        Default::default()
    }

    fn view_context(&self) -> ContextView<'_, DocumentListMessage> {
        // Sort Section
        let sort_section = widget::settings::section()
            .title(fl!("document-list-sort-by"))
            .add(
                iced::widget::pick_list(
                    DocumentSortOption::ALL,
                    Some(self.sort_option),
                    DocumentListMessage::SortChanged,
                )
                .width(Length::Fill),
            );

        // Source Filter Section
        let source_section = widget::settings::section()
            .title(fl!("document-list-filter-by-source"))
            .add(
                iced::widget::pick_list(
                    self.available_sources.clone(),
                    self.source_filter.clone(),
                    |source| DocumentListMessage::SourceFilterChanged(Some(source)),
                )
                .width(Length::Fill)
                .placeholder(fl!("document-list-all-sources")),
            )
            .add_maybe(self.source_filter.as_ref().map(|_| {
                widget::button::text(fl!("document-list-clear-filter"))
                    .on_press(DocumentListMessage::ClearSourceFilter)
            }));

        // Reading Status Filter Section
        let status_section = widget::settings::section()
            .title(fl!("document-list-filter-by-status"))
            .add(
                iced::widget::pick_list(
                    [
                        ReadingStatus::Unread,
                        ReadingStatus::Reading,
                        ReadingStatus::Read,
                    ],
                    self.status_filter,
                    |status| DocumentListMessage::StatusFilterChanged(Some(status)),
                )
                .width(Length::Fill)
                .placeholder(fl!("document-list-all-statuses")),
            )
            .add_maybe(self.status_filter.map(|_| {
                widget::button::text(fl!("document-list-clear-filter"))
                    .on_press(DocumentListMessage::ClearStatusFilter)
            }));

        ContextView {
            title: fl!("document-list-options-title"),
            content: widget::settings::view_column(vec![
                sort_section.into(),
                source_section.into(),
                status_section.into(),
                self.tag_filter.view().map(Into::into),
            ])
            .into(),
        }
    }

    fn update(&mut self, message: DocumentListMessage) -> Task<Action<DocumentListMessage>> {
        tracing::debug!("received: {message:?}");
        match message {
            DocumentListMessage::LoadArchive => {
                let document_provider = self.document_provider.clone();

                Task::batch([
                    self.archive
                        .set_document_state(DocumentState::Loading)
                        .map(ActionExt::map_into),
                    task::future({
                        let document_provider = document_provider.clone();
                        async move {
                            match document_provider.get_documents().await {
                                Ok(documents) => DocumentListMessage::Loaded(documents),
                                Err(error) => {
                                    DocumentListMessage::LoadingFailed(format!("{error}"))
                                }
                            }
                        }
                    }),
                    task::future(async move {
                        DocumentListMessage::SetAvailableSources(
                            document_provider.get_client_selectors().await,
                        )
                    }),
                    task::message(DocumentListMessage::TagFilter(TagFilterMessage::Tags(
                        ProvidedStateMessage::Load,
                    ))),
                ])
            }
            DocumentListMessage::Loaded(files) => {
                // For initial load, use synchronous filtering and sorting since it's typically fast
                let mut documents: Vec<Document> = files.into_iter().collect();
                // Sort documents
                sort_documents(&mut documents, self.sort_option);
                let mut files = Filtered::new(documents);
                files.filter(|file| {
                    filter_document(
                        &self.search_query,
                        self.status_filter,
                        self.source_filter.as_ref(),
                        &self.tag_filter.allow_tags,
                        &self.tag_filter.deny_tags,
                        &file,
                    )
                });

                let collection_size = files.filtered_len();
                Task::batch([
                    self.archive
                        .set_document_state(DocumentState::Loaded(files))
                        .map(ActionExt::map_into),
                    task::message(DocumentListMessage::DocumentsComponent(
                        DocumentsMessage::Pagination(PaginationMessage::SetCollectionSize(
                            collection_size,
                        )),
                    )),
                    task::message(DocumentListMessage::DocumentsComponent(
                        DocumentsMessage::FilterSelectedDocuments,
                    )),
                ])
            }
            DocumentListMessage::LoadingFailed(error) => self
                .archive
                .set_document_state(DocumentState::Failed(error))
                .map(ActionExt::map_into),
            DocumentListMessage::RefreshDocument(document) => {
                let document_fingerprint = document.metadata.fingerprint.clone();
                self.archive
                    .update_item(
                        move |doc| doc.metadata.fingerprint == document_fingerprint,
                        document,
                    )
                    .map(ActionExt::map_into)
            }
            DocumentListMessage::SearchChanged(query) => {
                self.search_query = query.clone();
                // Increment debounce counter to invalidate previous timers
                self.debounce_counter += 1;

                // Only start debounce timer if files have been loaded
                if self.archive.is_loaded() {
                    self.start_debounce_timer(self.debounce_counter, query)
                } else {
                    Task::none()
                }
            }
            DocumentListMessage::ClearSearch => {
                self.search_query.clear();
                // Immediately filter to show all files (no debounce needed for clearing)
                self.filter_now()
            }
            DocumentListMessage::FilteringComplete(filtered_files) => {
                let collection_size = filtered_files.len();
                self.is_filtering = false;
                self.archive.set_filtered_indices(filtered_files);
                task::message(DocumentListMessage::DocumentsComponent(
                    DocumentsMessage::Pagination(PaginationMessage::SetCollectionSize(
                        collection_size,
                    )),
                ))
            }
            DocumentListMessage::DebounceTimeout(counter, query) => {
                // Only proceed if this timeout matches the current counter (not superseded by newer typing)
                if self.archive.is_loaded()
                    && counter == self.debounce_counter
                    && !self.is_filtering
                {
                    self.is_filtering = true;
                    Task::batch(vec![
                        self.start_background_filtering(
                            query,
                            self.status_filter,
                            self.source_filter.clone(),
                            self.tag_filter.allow_tags.clone(),
                            self.tag_filter.deny_tags.clone(),
                            self.archive.unfiltered().to_vec(),
                        ),
                        task::message(DocumentListMessage::FocusSearchInput),
                    ])
                } else {
                    // This timeout was superseded by newer typing, ignore it
                    Task::none()
                }
            }
            DocumentListMessage::FocusSearchInput => {
                widget::text_input::focus(self.search_input_id.clone())
            }
            DocumentListMessage::SortChanged(sort_option) => {
                self.sort_option = sort_option;
                // Re-sort the documents immediately
                self.sort_now()
            }
            DocumentListMessage::StatusFilterChanged(status) => {
                self.status_filter = status;
                // Immediately filter with new status (no debounce needed for status changes)
                self.filter_now()
            }
            DocumentListMessage::ClearStatusFilter => {
                self.status_filter = None;
                // Immediately filter to show all statuses (no debounce needed for clearing)
                self.filter_now()
            }
            DocumentListMessage::SourceFilterChanged(source) => {
                self.source_filter = source;
                // Immediately filter with new source (no debounce needed for source changes)
                self.filter_now()
            }
            DocumentListMessage::ClearSourceFilter => {
                self.source_filter = None;
                // Immediately filter to show all sources (no debounce needed for clearing)
                self.filter_now()
            }
            DocumentListMessage::TagFilter(msg) => match msg {
                TagFilterMessage::Out(msg) => match msg {
                    TagFilterOutput::TagFiltersUpdated => {
                        // Immediately filter to show all statuses (no debounce needed for tag filter changes)
                        self.filter_now()
                    }
                },
                msg => self.tag_filter.update(msg).map(ActionExt::map_into),
            },
            DocumentListMessage::DocumentsComponent(msg) => match msg {
                DocumentsMessage::Out(msg) => match msg {
                    DocumentsOutput::DocumentClicked(file) => cosmic::task::message(
                        DocumentListMessage::Out(DocumentListOutput::OpenDetails(file)),
                    ),
                    DocumentsOutput::BatchTagEditor(msg) => {
                        self.handle_batch_tag_editor_output(msg)
                    }
                    DocumentsOutput::SelectionChanged => {
                        // Reset DocumentList's batch tag editor when selection changes
                        Task::none()
                    }
                },
                msg => self.archive.update(msg).map(ActionExt::map_into),
            },
            DocumentListMessage::SetAvailableSources(client_selectors) => {
                // Set available sources
                self.available_sources = client_selectors;
                // Clear source filter if it doesn't exist anymore
                if let Some(source) = &self.source_filter
                    && !self.available_sources.contains(source)
                {
                    self.source_filter = None;
                    // Immediately filter to reflect clearing the source filter (no debounce needed for clearing)
                    self.filter_now()
                } else {
                    Task::none()
                }
            }
            DocumentListMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent component")
            }
        }
    }
}
