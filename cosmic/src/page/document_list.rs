// SPDX-License-Identifier: GPL-3.0-or-later

use std::cmp::Ordering;
use std::collections::HashSet;
use std::fmt;
use std::slice;
use std::sync::Arc;

use cosmic::Action;
use cosmic::Application;
use cosmic::Apply;
use cosmic::Element;
use cosmic::Task;
use cosmic::cosmic_config;
use cosmic::cosmic_config::ConfigGet;
use cosmic::cosmic_config::ConfigSet;
use cosmic::iced;
use cosmic::iced::Length;
use cosmic::iced::keyboard::Key;
use cosmic::iced::keyboard::Modifiers;
use cosmic::iced::keyboard::key::Named;
use cosmic::task;
use cosmic::widget;
use read_flow_core::Builder;
use read_flow_core::api::ReadingStatus;
use regex::Regex;

use crate::aggregator::Document;
use crate::aggregator::Documents;
use crate::app::ContextView;
use crate::app::ReadFlow;
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
use crate::component::tag_pill_filter;
use crate::cosmic_ext::ActionExt;
use crate::document_provider::DocumentProvider;
use crate::fl;
use crate::layout::layout;
use crate::page::Page;
use crate::state::filtered::Filtered;

/// Search mode for the search box
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchMode {
    #[default]
    Fuzzy,
    Regex,
}

impl SearchMode {
    pub const ALL: &'static [Self] = &[Self::Fuzzy, Self::Regex];
}

impl fmt::Display for SearchMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Fuzzy => fl!("document-list-search-mode-fuzzy"),
            Self::Regex => fl!("document-list-search-mode-regex"),
        };
        write!(f, "{}", label)
    }
}

const DOC_LIST_PREFS_VERSION: u64 = 1;
const KEY_SORT_SUBJECT: &str = "sort_subject";
const KEY_SORT_DIRECTION: &str = "sort_direction";
const KEY_SEARCH_MODE: &str = "search_mode";

fn load_document_list_prefs() -> (SortSubject, SortDirection, SearchMode) {
    let Ok(ctx) = cosmic_config::Config::new(ReadFlow::APP_ID, DOC_LIST_PREFS_VERSION) else {
        return Default::default();
    };
    let sort_subject = if let Ok(s) = ctx.get::<String>(KEY_SORT_SUBJECT) {
        match s.as_str() {
            "title" => SortSubject::Title,
            "size" => SortSubject::Size,
            "type" => SortSubject::Type,
            "status" => SortSubject::Status,
            _ => SortSubject::default(),
        }
    } else {
        SortSubject::default()
    };
    let sort_direction = if let Ok(s) = ctx.get::<String>(KEY_SORT_DIRECTION) {
        match s.as_str() {
            "descending" => SortDirection::Descending,
            _ => SortDirection::default(),
        }
    } else {
        SortDirection::default()
    };
    let search_mode = if let Ok(s) = ctx.get::<String>(KEY_SEARCH_MODE) {
        match s.as_str() {
            "regex" => SearchMode::Regex,
            _ => SearchMode::default(),
        }
    } else {
        SearchMode::default()
    };
    (sort_subject, sort_direction, search_mode)
}

fn save_document_list_prefs(
    sort_subject: SortSubject,
    sort_direction: SortDirection,
    search_mode: SearchMode,
) {
    let Ok(ctx) = cosmic_config::Config::new(ReadFlow::APP_ID, DOC_LIST_PREFS_VERSION) else {
        return;
    };
    let subject_str = match sort_subject {
        SortSubject::Filename => "filename",
        SortSubject::Title => "title",
        SortSubject::Size => "size",
        SortSubject::Type => "type",
        SortSubject::Status => "status",
    };
    let _ = ctx.set(KEY_SORT_SUBJECT, subject_str);
    let direction_str = match sort_direction {
        SortDirection::Ascending => "ascending",
        SortDirection::Descending => "descending",
    };
    let _ = ctx.set(KEY_SORT_DIRECTION, direction_str);
    let mode_str = match search_mode {
        SearchMode::Fuzzy => "fuzzy",
        SearchMode::Regex => "regex",
    };
    let _ = ctx.set(KEY_SEARCH_MODE, mode_str);
}

/// Sort subject for the document list
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortSubject {
    #[default]
    Filename,
    Title,
    Size,
    Type,
    Status,
}

impl SortSubject {
    pub const ALL: &'static [Self] = &[
        Self::Filename,
        Self::Title,
        Self::Size,
        Self::Type,
        Self::Status,
    ];
}

impl fmt::Display for SortSubject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Filename => fl!("document-list-sort-filename"),
            Self::Title => fl!("document-list-sort-title"),
            Self::Size => fl!("document-list-sort-size"),
            Self::Type => fl!("document-list-sort-type"),
            Self::Status => fl!("document-list-sort-status"),
        };
        write!(f, "{}", label)
    }
}

/// Sort direction for the document list
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortDirection {
    #[default]
    Ascending,
    Descending,
}

impl SortDirection {
    pub fn toggle(self) -> Self {
        match self {
            Self::Ascending => Self::Descending,
            Self::Descending => Self::Ascending,
        }
    }
}

/// State for the merge-documents dialog.
struct MergeDialogState {
    candidates: Vec<Document>,
    /// Index into `candidates` for the chosen winner.
    winner_index: Option<usize>,
}

pub struct DocumentList {
    pub(super) document_provider: Arc<DocumentProvider>,
    archive: DocumentsComponent,
    is_filtering: bool,                   // Track if filtering is in progress
    search_query: String,                 // The search query string
    search_mode: SearchMode,              // Fuzzy or Regex search mode
    search_regex_error: Option<String>,   // Set when regex mode has an invalid pattern
    search_input_id: widget::Id,          // Unique ID for focus management
    debounce_counter: u32,                // Counter to track debounce state
    sort_subject: SortSubject,            // Current sort subject
    sort_direction: SortDirection,        // Current sort direction
    status_filter: Option<ReadingStatus>, // Optional reading status filter
    tag_filter: TagFilter<Arc<DocumentProvider>>, // Tag Filter component
    source_filter: Option<ClientSelector>, // Optional source filter
    available_sources: Vec<ClientSelector>, // Available sources for filtering
    pending_format_pick: Option<Document>, // Document awaiting format selection
    merge_dialog: Option<MergeDialogState>, // Merge dialog state
}

#[derive(Debug, Clone)]
pub enum DocumentListOutput {
    OpenDetails(Document),
    OpenDocument(Document),
    NavigateToSettings,
    Scan,
}

#[derive(Debug, Clone)]
pub enum DocumentListMessage {
    LoadArchive,
    Loaded(Documents),
    LoadingFailed(String),
    RefreshDocument(Document),
    SearchChanged(String),
    SearchModeChanged(SearchMode),
    ClearSearch,
    FilteringComplete(Vec<usize>),
    FocusSearchInput,
    DebounceTimeout(u32, String), // (counter, query) - triggers filtering after delay
    SortSubjectChanged(SortSubject),
    ToggleSortDirection,
    Key(Modifiers, Key),
    StatusFilterChanged(Option<ReadingStatus>),
    ClearStatusFilter,
    SourceFilterChanged(Option<ClientSelector>),
    ClearSourceFilter,
    TagFilter(TagFilterMessage),
    DocumentsComponent(DocumentsMessage),
    SetAvailableSources(Vec<ClientSelector>),
    PickDocumentSource(String),
    CancelFormatPick,
    OpenMergeDialog,
    MergeWinnerSelected(usize),
    ConfirmMerge,
    CancelMerge,
    MergeCompleted(Result<(), String>),
    Out(DocumentListOutput),
    CoversLoaded(std::collections::HashMap<String, Vec<u8>>),
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
        criteria: FilterCriteria,
        all_files: Vec<Document>,
    ) -> Task<Action<DocumentListMessage>> {
        task::future(async move {
            let compiled_regex =
                if criteria.search_mode == SearchMode::Regex && !criteria.query.is_empty() {
                    Regex::new(&criteria.query).ok()
                } else {
                    None
                };
            let filtered_files = all_files
                .iter()
                .enumerate()
                .filter_map(|(index, file)| {
                    filter_document(&criteria, compiled_regex.as_ref(), &file).then_some(index)
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

        let (sort_subject, sort_direction, search_mode) = load_document_list_prefs();

        (
            Self {
                document_provider: document_provider.clone(),
                archive,
                search_query: String::new(),
                search_mode,
                search_regex_error: None,
                is_filtering: false,
                search_input_id: widget::Id::unique(),
                debounce_counter: 0,
                sort_subject,
                sort_direction,
                status_filter: None,
                tag_filter,
                source_filter: None,
                available_sources: Default::default(),
                pending_format_pick: None,
                merge_dialog: None,
            },
            Task::batch(vec![
                tag_filter_init.map(ActionExt::map_into),
                archive_init.map(ActionExt::map_into),
                task::message(DocumentListMessage::LoadArchive),
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
                FilterCriteria {
                    query: self.search_query.clone(),
                    search_mode: self.search_mode,
                    status_filter: self.status_filter,
                    source_filter: self.source_filter.clone(),
                    allow_tags: self.tag_filter.allow_tags.clone(),
                    deny_tags: self.tag_filter.deny_tags.clone(),
                },
                self.archive.unfiltered().to_vec(),
            )
        } else {
            Task::none()
        }
    }

    fn sort_now(&mut self) -> Task<Action<DocumentListMessage>> {
        if self.archive.is_loaded() {
            // Sort the unfiltered documents in place
            let sort_subject = self.sort_subject;
            let sort_direction = self.sort_direction;
            self.archive
                .sort_unfiltered(|docs| sort_documents(docs, sort_subject, sort_direction));
            // Re-apply the current filter to update the filtered view
            self.filter_now()
        } else {
            Task::none()
        }
    }
}

impl DocumentList {
    fn view_merge_dialog(&self, dialog: &MergeDialogState) -> Element<'_, DocumentListMessage> {
        use cosmic::cosmic_theme::Spacing;
        use cosmic::theme;

        let Spacing { space_s, .. } = theme::active().cosmic().spacing;

        let candidate_rows: Vec<Element<'_, DocumentListMessage>> = dialog
            .candidates
            .iter()
            .enumerate()
            .map(|(idx, doc)| {
                let label = doc
                    .user_meta
                    .title
                    .as_deref()
                    .map(str::to_owned)
                    .unwrap_or_else(|| {
                        doc.local_or_any_source()
                            .and_then(|(_, s)| std::path::Path::new(&s.path).file_name()?.to_str())
                            .unwrap_or("")
                            .to_owned()
                    });
                widget::settings::item_row(vec![])
                    .push(widget::radio(
                        widget::text::body(label),
                        idx,
                        dialog.winner_index,
                        DocumentListMessage::MergeWinnerSelected,
                    ))
                    .into()
            })
            .collect();

        let controls = widget::column::with_children(candidate_rows)
            .spacing(space_s)
            .apply(widget::container)
            .class(theme::Container::Card)
            .padding(space_s)
            .width(iced::Length::Fill);

        let merge_btn = cosmic::widget::button::suggested(fl!("document-list-merge-confirm"))
            .apply_maybe(dialog.winner_index.is_some().then_some(()), |btn, _| {
                btn.on_press(DocumentListMessage::ConfirmMerge)
            });

        cosmic::widget::dialog()
            .title(fl!("document-list-merge-title"))
            .body(fl!("document-list-merge-body"))
            .control(controls)
            .primary_action(merge_btn)
            .secondary_action(
                cosmic::widget::button::standard(fl!("document-list-merge-cancel"))
                    .on_press(DocumentListMessage::CancelMerge),
            )
            .into()
    }
}

fn shortcut_item(key: &str, description: String) -> Element<'_, DocumentListMessage> {
    widget::settings::item::builder(description)
        .control(widget::text::monotext(key))
        .into()
}

fn sort_documents(
    documents: &mut [Document],
    sort_subject: SortSubject,
    sort_direction: SortDirection,
) {
    documents.sort_by(|a, b| compare_documents(a, b, sort_subject, sort_direction));
}

/// Compare two documents based on the sort option
fn compare_documents(
    a: &Document,
    b: &Document,
    sort_subject: SortSubject,
    sort_direction: SortDirection,
) -> Ordering {
    let ordering = match sort_subject {
        SortSubject::Filename => get_filename(a)
            .to_lowercase()
            .cmp(&get_filename(b).to_lowercase()),
        SortSubject::Title => get_display_title(a)
            .to_lowercase()
            .cmp(&get_display_title(b).to_lowercase()),
        SortSubject::Size => {
            let a_size = a.contents.first().map(|c| c.size).unwrap_or(0);
            let b_size = b.contents.first().map(|c| c.size).unwrap_or(0);
            a_size.cmp(&b_size)
        }
        SortSubject::Type => {
            let a_type = a.contents.first().map(|c| c.type_.as_str()).unwrap_or("");
            let b_type = b.contents.first().map(|c| c.type_.as_str()).unwrap_or("");
            a_type.cmp(b_type)
        }
        SortSubject::Status => {
            let a_status = a
                .contents
                .first()
                .map(|c| c.status)
                .unwrap_or(ReadingStatus::Unread);
            let b_status = b
                .contents
                .first()
                .map(|c| c.status)
                .unwrap_or(ReadingStatus::Unread);
            status_order(&a_status).cmp(&status_order(&b_status))
        }
    };

    match sort_direction {
        SortDirection::Ascending => ordering,
        SortDirection::Descending => ordering.reverse(),
    }
}

/// Get the filename from a document (uses local source if available, otherwise any source)
fn get_filename(doc: &Document) -> &str {
    if let Some((_, source)) = doc.local_or_any_source() {
        source.path.rsplit('/').next().unwrap_or(&source.path)
    } else {
        ""
    }
}

/// Get the display title: user-edited title if set, otherwise the filename.
fn get_display_title(doc: &Document) -> &str {
    doc.user_meta
        .title
        .as_deref()
        .unwrap_or_else(|| get_filename(doc))
}

/// Convert reading status to a sortable order (Unread=0, Reading=1, Read=2)
fn status_order(status: &ReadingStatus) -> u8 {
    match status {
        ReadingStatus::Unread => 0,
        ReadingStatus::Reading => 1,
        ReadingStatus::Read => 2,
    }
}

/// Returns the regex compile error message if mode is Regex and the query is invalid.
fn compute_regex_error(mode: SearchMode, query: &str) -> Option<String> {
    if mode == SearchMode::Regex && !query.is_empty() {
        Regex::new(query).err().map(|e| e.to_string())
    } else {
        None
    }
}

/// Returns true if every character of `query` appears in `text` as a subsequence
/// (in order, but not necessarily consecutive).
fn fuzzy_match(query: &str, text: &str) -> bool {
    let mut text_chars = text.chars();
    for q in query.chars() {
        if !text_chars.any(|t| t == q) {
            return false;
        }
    }
    true
}

struct FilterCriteria {
    query: String,
    search_mode: SearchMode,
    status_filter: Option<ReadingStatus>,
    source_filter: Option<ClientSelector>,
    allow_tags: HashSet<String>,
    deny_tags: HashSet<String>,
}

fn filter_document(
    criteria: &FilterCriteria,
    compiled_regex: Option<&Regex>,
    document: &&Document,
) -> bool {
    let search_query = criteria.query.as_str();
    let search_mode = criteria.search_mode;
    let status_filter = criteria.status_filter;
    let source_filter = criteria.source_filter.as_ref();
    let allow_tags = &criteria.allow_tags;
    let deny_tags = &criteria.deny_tags;

    let all_tags: Vec<String> = document
        .contents
        .iter()
        .flat_map(|c| c.tags.iter().cloned())
        .collect();

    let matches_search = if search_query.is_empty() {
        true
    } else {
        match search_mode {
            SearchMode::Fuzzy => {
                let query = search_query.to_lowercase();
                let path_matches = document
                    .contents
                    .iter()
                    .flat_map(|c| c.sources.iter())
                    .map(|source| source.path.to_lowercase())
                    .filter(|path| fuzzy_match(&query, path))
                    .count();
                let tags_lower = all_tags.join(" ").to_lowercase();
                let title_lower = document
                    .user_meta
                    .title
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase();
                let authors_lower = document
                    .user_meta
                    .authors
                    .as_deref()
                    .unwrap_or(&[])
                    .join(" ")
                    .to_lowercase();
                path_matches > 0
                    || fuzzy_match(&query, &tags_lower)
                    || fuzzy_match(&query, &title_lower)
                    || fuzzy_match(&query, &authors_lower)
            }
            SearchMode::Regex => {
                if let Some(re) = compiled_regex {
                    let path_matches = document
                        .contents
                        .iter()
                        .flat_map(|c| c.sources.iter())
                        .any(|source| re.is_match(&source.path));
                    let tags = all_tags.join(" ");
                    let title = document.user_meta.title.as_deref().unwrap_or("");
                    let authors = document
                        .user_meta
                        .authors
                        .as_deref()
                        .unwrap_or(&[])
                        .join(" ");
                    path_matches
                        || re.is_match(&tags)
                        || re.is_match(title)
                        || re.is_match(&authors)
                } else {
                    // Invalid or empty regex: show all results
                    true
                }
            }
        }
    };

    // Filter by reading status (match any content's status)
    let matches_status =
        status_filter.is_none_or(|status| document.contents.iter().any(|c| c.status == status));

    // Filter by source (document must exist on the selected source)
    let matches_source = source_filter.is_none_or(|source| {
        document
            .contents
            .iter()
            .flat_map(|c| c.sources.iter())
            .any(|doc_source| &doc_source.client == source)
    });

    // Filter by allowed tags (file must have ALL allowed tags)
    let matches_allow_tags =
        allow_tags.is_empty() || allow_tags.iter().all(|tag| all_tags.contains(tag));

    // Filter by denied tags (file must have NONE of the denied tags)
    let matches_deny_tags =
        deny_tags.is_empty() || !all_tags.iter().any(|tag| deny_tags.contains(tag));

    matches_search && matches_status && matches_source && matches_allow_tags && matches_deny_tags
}

impl Page for DocumentList {
    type Message = DocumentListMessage;

    fn dialog(&self) -> Option<Element<'_, DocumentListMessage>> {
        if let Some(dialog) = &self.merge_dialog {
            return Some(self.view_merge_dialog(dialog));
        }

        let document = self.pending_format_pick.as_ref()?;

        let title = document.user_meta.title.clone().unwrap_or_else(|| {
            document
                .local_or_any_source()
                .and_then(|(_, s)| std::path::Path::new(&s.path).file_stem()?.to_str())
                .unwrap_or("")
                .to_owned()
        });

        let mut sources = document.sources_by_priority();
        sources.sort_by(|(ac, as_), (bc, bs)| {
            ac.type_
                .as_str()
                .cmp(bc.type_.as_str())
                .then_with(|| as_.client.is_local().cmp(&bs.client.is_local()))
        });

        // Collect per-content covers for this document so each row shows its own thumbnail.
        let doc_covers: std::collections::HashMap<String, widget::image::Handle> = document
            .contents
            .iter()
            .filter_map(|c| {
                self.archive
                    .covers()
                    .get(&c.fingerprint)
                    .map(|h| (c.fingerprint.clone(), h.clone()))
            })
            .collect();

        Some(crate::component::source_picker::source_picker_dialog(
            fl!("document-list-pick-source-title"),
            Some(title),
            sources,
            doc_covers,
            DocumentListMessage::PickDocumentSource,
            DocumentListMessage::CancelFormatPick,
        ))
    }

    fn view(&self) -> Element<'_, DocumentListMessage> {
        let tag_bar = tag_pill_filter::view(
            self.tag_filter.all_tags(),
            &self.tag_filter.allow_tags,
            &self.tag_filter.deny_tags,
        )
        .map(Into::into);

        widget::column::with_children(vec![
            layout(tag_bar),
            self.archive
                .view()
                .map(Into::into)
                .apply(widget::scrollable::vertical)
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
        ])
        .height(Length::Fill)
        .into()
    }

    fn view_header_center(&self) -> Vec<Element<'_, DocumentListMessage>> {
        let search_input = {
            widget::search_input(fl!("document-list-search-placeholder"), &self.search_query)
                .id(self.search_input_id.clone())
                .on_input(DocumentListMessage::SearchChanged)
                .on_clear(DocumentListMessage::ClearSearch)
                .width(Length::Fixed(300.0))
                .apply_maybe(self.search_regex_error.as_ref(), |input, err| {
                    input.error(err.as_str())
                })
        };

        let mut elements: Vec<Element<'_, DocumentListMessage>> = vec![search_input.into()];

        if self.is_filtering {
            elements.push(widget::text(fl!("document-list-filtering")).size(12).into());
        }

        elements
    }

    fn view_context(&self) -> ContextView<'_, DocumentListMessage> {
        // Search Mode Section
        let search_mode_section = widget::settings::section()
            .title(fl!("document-list-search-mode"))
            .add(
                iced::widget::pick_list(
                    SearchMode::ALL,
                    Some(self.search_mode),
                    DocumentListMessage::SearchModeChanged,
                )
                .width(Length::Fill),
            );

        // Sort Section
        let sort_icon = match self.sort_direction {
            SortDirection::Ascending => "view-sort-ascending-symbolic",
            SortDirection::Descending => "view-sort-descending-symbolic",
        };

        let sort_section = widget::settings::section()
            .title(fl!("document-list-sort-by"))
            .add(
                widget::Row::new()
                    .spacing(8)
                    .push(
                        iced::widget::pick_list(
                            SortSubject::ALL,
                            Some(self.sort_subject),
                            DocumentListMessage::SortSubjectChanged,
                        )
                        .width(Length::Fill),
                    )
                    .push(
                        widget::button::icon(widget::icon::from_name(sort_icon))
                            .on_press(DocumentListMessage::ToggleSortDirection),
                    ),
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

        let shortcuts_section = widget::settings::section()
            .title(fl!("document-list-keyboard-shortcuts"))
            .add(shortcut_item(
                "Ctrl+M",
                fl!("document-list-shortcut-toggle-search-mode"),
            ))
            .add(shortcut_item(
                "← ↑ PgUp",
                fl!("document-list-shortcut-previous-page"),
            ))
            .add(shortcut_item(
                "→ ↓ PgDn",
                fl!("document-list-shortcut-next-page"),
            ))
            .add(shortcut_item(
                "Home",
                fl!("document-list-shortcut-first-page"),
            ))
            .add(shortcut_item(
                "End",
                fl!("document-list-shortcut-last-page"),
            ));

        ContextView {
            title: fl!("document-list-options-title"),
            content: widget::settings::view_column(vec![
                search_mode_section.into(),
                sort_section.into(),
                source_section.into(),
                status_section.into(),
                self.tag_filter.view().map(Into::into),
                shortcuts_section.into(),
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
            DocumentListMessage::CoversLoaded(cover_bytes_map) => {
                let handles: std::collections::HashMap<String, widget::image::Handle> =
                    cover_bytes_map
                        .into_iter()
                        .map(|(fp, bytes)| (fp, widget::image::Handle::from_bytes(bytes)))
                        .collect();
                self.archive.set_covers(handles);
                Task::none()
            }
            DocumentListMessage::Loaded(files) => {
                // For initial load, use synchronous filtering and sorting since it's typically fast
                let mut documents: Vec<Document> = files.into_iter().collect();
                // Sort documents
                sort_documents(&mut documents, self.sort_subject, self.sort_direction);
                let criteria = FilterCriteria {
                    query: self.search_query.clone(),
                    search_mode: self.search_mode,
                    status_filter: self.status_filter,
                    source_filter: self.source_filter.clone(),
                    allow_tags: self.tag_filter.allow_tags.clone(),
                    deny_tags: self.tag_filter.deny_tags.clone(),
                };
                let compiled_regex =
                    if criteria.search_mode == SearchMode::Regex && !criteria.query.is_empty() {
                        Regex::new(&criteria.query).ok()
                    } else {
                        None
                    };
                let mut files = Filtered::new(documents);
                files.filter(|file| filter_document(&criteria, compiled_regex.as_ref(), &file));

                let collection_size = files.filtered_len();

                // Collect ALL fingerprints across all contents so per-content covers are loaded.
                let fingerprints: Vec<String> = files
                    .unfiltered()
                    .iter()
                    .flat_map(|doc| doc.contents.iter().map(|c| c.fingerprint.clone()))
                    .collect();
                let document_provider = self.document_provider.clone();
                let load_covers_task = task::future(async move {
                    let cover_bytes = document_provider.load_covers(fingerprints).await;
                    DocumentListMessage::CoversLoaded(cover_bytes)
                });

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
                    load_covers_task,
                ])
            }
            DocumentListMessage::LoadingFailed(error) => self
                .archive
                .set_document_state(DocumentState::Failed(error))
                .map(ActionExt::map_into),
            DocumentListMessage::RefreshDocument(document) => {
                let document_guid = document.document_guid.clone();
                self.archive
                    .update_item(move |doc| doc.document_guid == document_guid, document)
                    .map(ActionExt::map_into)
            }
            DocumentListMessage::SearchChanged(query) => {
                self.search_query = query.clone();
                // Increment debounce counter to invalidate previous timers
                self.debounce_counter += 1;

                self.search_regex_error = compute_regex_error(self.search_mode, &query);

                // Only start debounce timer if files have been loaded
                if self.archive.is_loaded() {
                    self.start_debounce_timer(self.debounce_counter, query)
                } else {
                    Task::none()
                }
            }
            DocumentListMessage::SearchModeChanged(mode) => {
                self.search_mode = mode;
                self.search_regex_error = compute_regex_error(mode, &self.search_query);
                save_document_list_prefs(self.sort_subject, self.sort_direction, self.search_mode);
                self.filter_now()
            }
            DocumentListMessage::ClearSearch => {
                self.search_query.clear();
                self.search_regex_error = None;
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
                    Task::batch(vec![self.start_background_filtering(
                        FilterCriteria {
                            query,
                            search_mode: self.search_mode,
                            status_filter: self.status_filter,
                            source_filter: self.source_filter.clone(),
                            allow_tags: self.tag_filter.allow_tags.clone(),
                            deny_tags: self.tag_filter.deny_tags.clone(),
                        },
                        self.archive.unfiltered().to_vec(),
                    )])
                } else {
                    // This timeout was superseded by newer typing, ignore it
                    Task::none()
                }
            }
            DocumentListMessage::FocusSearchInput => {
                widget::text_input::focus(self.search_input_id.clone())
            }
            DocumentListMessage::SortSubjectChanged(sort_subject) => {
                self.sort_subject = sort_subject;
                save_document_list_prefs(self.sort_subject, self.sort_direction, self.search_mode);
                self.sort_now()
            }
            DocumentListMessage::ToggleSortDirection => {
                self.sort_direction = self.sort_direction.toggle();
                save_document_list_prefs(self.sort_subject, self.sort_direction, self.search_mode);
                self.sort_now()
            }
            DocumentListMessage::Key(modifiers, key) => {
                if modifiers.control() && matches!(&key, Key::Character(c) if c.as_str() == "m") {
                    let next_mode = match self.search_mode {
                        SearchMode::Fuzzy => SearchMode::Regex,
                        SearchMode::Regex => SearchMode::Fuzzy,
                    };
                    task::message(DocumentListMessage::SearchModeChanged(next_mode))
                } else {
                    match key {
                        Key::Named(Named::ArrowLeft | Named::ArrowUp | Named::PageUp) => {
                            task::message(DocumentListMessage::DocumentsComponent(
                                DocumentsMessage::Pagination(
                                    PaginationMessage::NavigateToPreviousPage,
                                ),
                            ))
                        }
                        Key::Named(Named::ArrowRight | Named::ArrowDown | Named::PageDown) => {
                            task::message(DocumentListMessage::DocumentsComponent(
                                DocumentsMessage::Pagination(PaginationMessage::NavigateToNextPage),
                            ))
                        }
                        Key::Named(Named::Home) => task::message(
                            DocumentListMessage::DocumentsComponent(DocumentsMessage::Pagination(
                                PaginationMessage::NavigateToFirstPage,
                            )),
                        ),
                        Key::Named(Named::End) => {
                            task::message(DocumentListMessage::DocumentsComponent(
                                DocumentsMessage::Pagination(PaginationMessage::NavigateToLastPage),
                            ))
                        }
                        _ => Task::none(),
                    }
                }
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
                    DocumentsOutput::OpenDocumentDetails(document) => task::message(
                        DocumentListMessage::Out(DocumentListOutput::OpenDetails(document)),
                    ),
                    DocumentsOutput::BatchTagEditor(msg) => {
                        self.handle_batch_tag_editor_output(msg)
                    }
                    DocumentsOutput::SelectionChanged => {
                        // Reset DocumentList's batch tag editor when selection changes
                        Task::none()
                    }
                    DocumentsOutput::OpenDocument(document) => task::message(
                        DocumentListMessage::Out(DocumentListOutput::OpenDocument(document)),
                    ),
                    DocumentsOutput::PickFormat(document) => {
                        self.pending_format_pick = Some(document);
                        Task::none()
                    }
                    DocumentsOutput::NavigateToSettings => task::message(DocumentListMessage::Out(
                        DocumentListOutput::NavigateToSettings,
                    )),
                    DocumentsOutput::Scan => {
                        task::message(DocumentListMessage::Out(DocumentListOutput::Scan))
                    }
                    DocumentsOutput::MergeDocuments => {
                        task::message(DocumentListMessage::OpenMergeDialog)
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
            DocumentListMessage::PickDocumentSource(guid) => {
                if let Some(doc) = self.pending_format_pick.take()
                    && let Some(single) = doc.with_source_guid(&guid)
                {
                    return task::message(DocumentListMessage::Out(
                        DocumentListOutput::OpenDocument(single),
                    ));
                }
                Task::none()
            }
            DocumentListMessage::OpenMergeDialog => {
                let candidates = self.archive.get_selected_documents();
                if candidates.len() >= 2 {
                    self.merge_dialog = Some(MergeDialogState {
                        candidates,
                        winner_index: None,
                    });
                }
                Task::none()
            }
            DocumentListMessage::MergeWinnerSelected(index) => {
                if let Some(ref mut dialog) = self.merge_dialog {
                    dialog.winner_index = Some(index);
                }
                Task::none()
            }
            DocumentListMessage::ConfirmMerge => {
                let Some(dialog) = self.merge_dialog.take() else {
                    return Task::none();
                };
                let Some(winner_idx) = dialog.winner_index else {
                    return Task::none();
                };
                let Some(winner) = dialog.candidates.get(winner_idx).cloned() else {
                    return Task::none();
                };
                let winner_guid = winner.document_guid.clone();
                let losers: Vec<Document> = dialog
                    .candidates
                    .into_iter()
                    .filter(|d| d.document_guid != winner_guid)
                    .collect();
                let provider = self.document_provider.clone();
                task::future(async move {
                    let result = provider.merge_documents(&winner, &losers).await;
                    DocumentListMessage::MergeCompleted(result.map_err(|e| e.to_string()))
                })
            }
            DocumentListMessage::CancelMerge => {
                self.merge_dialog = None;
                Task::none()
            }
            DocumentListMessage::MergeCompleted(result) => {
                if let Err(ref e) = result {
                    tracing::warn!("merge documents failed: {e}");
                }
                task::message(DocumentListMessage::LoadArchive)
            }
            DocumentListMessage::CancelFormatPick => {
                self.pending_format_pick = None;
                Task::none()
            }
            DocumentListMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent component")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::fuzzy_match;

    #[rstest]
    #[case("rust", "rust-programming", true)]
    #[case("rsp", "rust-programming", true)]
    #[case("rpg", "rust-programming", true)]
    #[case("xyz", "rust-programming", false)]
    #[case("", "rust-programming", true)]
    #[case("rust", "", false)]
    #[case("RUST", "rust-programming", false)] // case-sensitive: caller lowercases both
    fn test_fuzzy_match(#[case] query: &str, #[case] text: &str, #[case] expected: bool) {
        assert_eq!(fuzzy_match(query, text), expected);
    }
}
