// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashSet;

use archive_organizer::api::ReadingStatus;
use cosmic::Action;
use cosmic::Apply;
use cosmic::Element;
use cosmic::Task;
use cosmic::cosmic_theme;
use cosmic::iced;
use cosmic::iced::Length;
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::alignment::Vertical;
use cosmic::task;
use cosmic::theme;
use cosmic::widget;
use cosmic::widget::settings;

use crate::aggregator::Aggregator;
use crate::aggregator::Document;
use crate::aggregator::Documents;
use crate::app::ContextView;
use crate::component::documents::DocumentState;
use crate::component::documents::DocumentsComponent;
use crate::component::documents::DocumentsMessage;
use crate::component::documents::DocumentsOutput;
use crate::component::pagination::PaginationMessage;
use crate::component::tag_filter::TagFilter;
use crate::component::tag_filter::TagFilterMessage;
use crate::component::tag_filter::TagFilterOutput;
use crate::cosmic_ext::ActionExt;
use crate::fl;
use crate::state::filtered::Filtered;

pub struct DocumentList {
    pub(super) aggregator: Aggregator,
    archive: DocumentsComponent,
    is_filtering: bool,                   // Track if filtering is in progress
    search_query: String,                 // The search query string
    search_input_id: widget::Id,          // Unique ID for focus management
    debounce_counter: u32,                // Counter to track debounce state
    status_filter: Option<ReadingStatus>, // Optional reading status filter
    tag_filter: TagFilter,                // Tag Filter component
}

#[derive(Debug, Clone)]
pub enum DocumentListOutput {
    OpenDetails(Document),
    ToggleContextPage,
}

#[derive(Debug, Clone)]
pub enum DocumentListMessage {
    LoadArchive,
    Loaded(Documents),
    LoadingFailed(String),
    RefreshFile(Document),
    SearchChanged(String),
    ClearSearch,
    FilteringComplete(Vec<usize>),
    FocusSearchInput,
    DebounceTimeout(u32, String), // (counter, query) - triggers filtering after delay
    StatusFilterChanged(Option<ReadingStatus>),
    ClearStatusFilter,
    TagFilter(TagFilterMessage),
    DocumentsComponent(DocumentsMessage),
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
                    filter_document(&query, status_filter, &allow_tags, &deny_tags, &file)
                        .then_some(index)
                })
                .collect();

            DocumentListMessage::FilteringComplete(filtered_files)
        })
    }

    pub fn new(aggregator: Aggregator) -> (Self, Task<Action<DocumentListMessage>>) {
        let aggregator_clone = aggregator.clone();
        let tags_fetcher = Box::new(move || {
            let agg = aggregator_clone.clone();
            Box::pin(async move { agg.get_file_tags().await.map_err(|e| format!("{e}")) })
                as std::pin::Pin<
                    Box<dyn std::future::Future<Output = Result<Vec<String>, String>> + Send>,
                >
        });
        let (tag_filter, tag_filter_init) = TagFilter::new(tags_fetcher);
        (
            Self {
                aggregator,
                archive: DocumentsComponent::default(),
                search_query: String::new(),
                is_filtering: false,
                search_input_id: widget::Id::unique(),
                debounce_counter: 0,
                status_filter: None,
                tag_filter,
            },
            Task::batch(vec![
                tag_filter_init.map(ActionExt::map_into),
                task::message(DocumentListMessage::LoadArchive),
                task::message(DocumentListMessage::FocusSearchInput),
            ]),
        )
    }

    pub fn view(&self) -> Element<'_, DocumentListMessage> {
        let cosmic_theme::Spacing {
            space_xxs, space_s, ..
        } = theme::active().cosmic().spacing;

        let column = widget::column().spacing(space_xxs);

        let header_row = widget::row().align_y(Vertical::Center).spacing(space_s);

        let header_row = header_row.push(
            widget::button::icon(widget::icon::from_name("open-menu-symbolic"))
                .on_press(DocumentListMessage::Out(
                    DocumentListOutput::ToggleContextPage,
                ))
                .apply(widget::container)
                .width(Length::Shrink)
                .height(Length::Shrink)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Center),
        );

        let search_input =
            widget::text_input(fl!("file-list-search-placeholder"), &self.search_query)
                .id(self.search_input_id.clone())
                .always_active()
                .on_input(DocumentListMessage::SearchChanged)
                .width(Length::FillPortion(2));

        let header_row = header_row.push(
            search_input
                .apply(widget::container)
                .height(Length::Shrink)
                .align_x(Horizontal::Left)
                .align_y(Vertical::Center),
        );

        let header_row = header_row.push(
            widget::button::icon(widget::icon::from_name("edit-clear-symbolic"))
                .on_press(DocumentListMessage::ClearSearch)
                .apply(widget::container)
                .width(Length::Shrink)
                .height(Length::Shrink)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Center),
        );

        let header_row = if self.is_filtering {
            // Show filtering indicator in the header
            header_row.push(
                widget::text(fl!("file-list-filtering"))
                    .size(12)
                    .apply(widget::container)
                    .width(Length::Shrink)
                    .height(Length::Shrink)
                    .align_x(Horizontal::Center)
                    .align_y(Vertical::Center),
            )
        } else {
            header_row
        };

        let header_row = header_row.push(
            widget::horizontal_space()
                .apply(widget::container)
                .width(Length::FillPortion(1))
                .height(Length::Shrink)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Center),
        );

        let column = column.push(
            header_row
                .apply(widget::container)
                .width(Length::Fill)
                .height(Length::Shrink)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Center),
        );

        let column = column.push(
            self.archive
                .view()
                .map(Into::into)
                .apply(widget::container)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Horizontal::Left)
                .align_y(Vertical::Top),
        );

        column.into()
    }

    pub fn view_context(&self) -> ContextView<'_, DocumentListMessage> {
        // Reading Status Filter Section
        let status_section = settings::section()
            .title(fl!("file-list-filter-by-status"))
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
                .placeholder(fl!("file-list-all-statuses")),
            )
            .add_maybe(self.status_filter.map(|_| {
                widget::button::text(fl!("file-list-clear-filter"))
                    .on_press(DocumentListMessage::ClearStatusFilter)
            }));

        ContextView {
            title: fl!("file-list-options-title"),
            content: settings::view_column(vec![
                status_section.into(),
                self.tag_filter.view().map(Into::into),
            ])
            .into(),
        }
    }

    pub fn update(&mut self, message: DocumentListMessage) -> Task<Action<DocumentListMessage>> {
        tracing::debug!("received: {message:?}");
        match message {
            DocumentListMessage::LoadArchive => {
                self.archive.documents = DocumentState::Loading;
                let aggregator = self.aggregator.clone();
                task::future({
                    let aggregator = aggregator.clone();
                    async move {
                        match aggregator.aggregate().await {
                            Ok(documents) => DocumentListMessage::Loaded(documents),
                            Err(error) => DocumentListMessage::LoadingFailed(format!("{error}")),
                        }
                    }
                })
            }
            DocumentListMessage::Loaded(files) => {
                // For initial load, use synchronous filtering since it's typically fast
                let mut files = Filtered::new(files.into_iter().collect());
                files.filter(|file| {
                    filter_document(
                        &self.search_query,
                        self.status_filter,
                        &self.tag_filter.allow_tags,
                        &self.tag_filter.deny_tags,
                        &file,
                    )
                });

                let collection_size = files.filtered_len();
                self.archive.documents = DocumentState::Loaded(files);
                task::message(DocumentListMessage::DocumentsComponent(
                    DocumentsMessage::Pagination(PaginationMessage::SetCollectionSize(
                        collection_size,
                    )),
                ))
            }
            DocumentListMessage::LoadingFailed(error) => {
                self.archive.documents = DocumentState::Failed(error);
                Task::none()
            }
            DocumentListMessage::RefreshFile(_file) => {
                // self.archive
                //     .documents
                //     .unwrap_mut()
                //     .update_item(move |old_file| old_file.id == file.id, file);
                Task::none()
            }
            DocumentListMessage::SearchChanged(query) => {
                self.search_query = query.clone();
                // Increment debounce counter to invalidate previous timers
                self.debounce_counter += 1;

                // Only start debounce timer if files have been loaded
                if self.archive.documents.is_loaded() {
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
                if self.archive.documents.is_loaded()
                    && counter == self.debounce_counter
                    && !self.is_filtering
                {
                    self.is_filtering = true;
                    Task::batch(vec![
                        self.start_background_filtering(
                            query,
                            self.status_filter,
                            self.tag_filter.allow_tags.clone(),
                            self.tag_filter.deny_tags.clone(),
                            self.archive.documents.unwrap().unfiltered().to_vec(),
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
                },
                msg => self.archive.update(msg).map(ActionExt::map_into),
            },
            DocumentListMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent component")
            }
        }
    }

    fn filter_now(&mut self) -> Task<Action<DocumentListMessage>> {
        // Increment debounce counter to invalidate previous timers
        self.debounce_counter += 1;

        if self.archive.documents.is_loaded() && !self.is_filtering {
            self.is_filtering = true;
            self.start_background_filtering(
                self.search_query.clone(),
                self.status_filter,
                self.tag_filter.allow_tags.clone(),
                self.tag_filter.deny_tags.clone(),
                self.archive.documents.unwrap().unfiltered().to_vec(),
            )
        } else {
            Task::none()
        }
    }
}

fn filter_document(
    search_query: &str,
    status_filter: Option<ReadingStatus>,
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

    matches_search && matches_status && matches_allow_tags && matches_deny_tags
}
