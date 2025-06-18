// SPDX-License-Identifier: GPL-3.0-or-later

use crate::app::ContextView;
use crate::client::{Client, ClientSelector};
use crate::fl;
use crate::state::LoadedState;
use archive_organizer::api::{File, FileDataSource, ReadingStatus};
use cosmic::iced::Length;
use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::iced::widget::combo_box;
use cosmic::iced_widget::list::Content;
use cosmic::widget;
use cosmic::{Apply, Element, Task};
use std::collections::HashSet;
use std::path::Path;

struct Files {
    all_files: Vec<File>,
    visible_files: Content<File>,
}

impl Files {
    fn new(files: Vec<File>) -> Self {
        Self {
            all_files: files.clone(),
            visible_files: Content::with_items(files),
        }
    }

    fn set_visible(&mut self, files: Vec<File>) {
        self.visible_files = Content::with_items(files);
    }

    /// Filter files based on the search query, reading status, and tags (synchronous version for initial load only)
    fn filtered_by(
        mut self,
        search_query: &str,
        status_filter: Option<ReadingStatus>,
        allow_tags: &HashSet<String>,
        deny_tags: &HashSet<String>,
    ) -> Self {
        let filtered_files = self
            .all_files
            .iter()
            .filter(|file| {
                // Filter by search query
                let matches_search = if search_query.is_empty() {
                    true
                } else {
                    let query = search_query.to_lowercase();
                    let path_lower = file.path.to_lowercase();
                    let tags_lower = file.tags.join(" ").to_lowercase();
                    path_lower.contains(&query) || tags_lower.contains(&query)
                };

                // Filter by reading status
                let matches_status = status_filter.map_or(true, |status| file.status == status);

                // Filter by allowed tags (file must have ALL allowed tags)
                let matches_allow_tags =
                    allow_tags.is_empty() || allow_tags.iter().all(|tag| file.tags.contains(tag));

                // Filter by denied tags (file must have NONE of the denied tags)
                let matches_deny_tags =
                    deny_tags.is_empty() || !file.tags.iter().any(|tag| deny_tags.contains(tag));

                matches_search && matches_status && matches_allow_tags && matches_deny_tags
            })
            .cloned()
            .collect();
        self.set_visible(filtered_files);
        self
    }
}

type FileState = LoadedState<Files>;

impl FileState {
    pub fn view(&self) -> Element<FileListMessage> {
        match self {
            FileState::New => widget::text(fl!("file-list-new")).into(), // TODO: Show spinner
            FileState::Loading => widget::text(fl!("file-list-loading")).into(), // TODO: Show spinner
            FileState::Failed(error) => {
                widget::text(fl!("generic-error", error = error.as_str())).into()
            }
            FileState::Loaded(files) => {
                let list = cosmic::iced::widget::list(&files.visible_files, |_index, file| {
                    view_file(file)
                })
                .spacing(10);
                list.apply(widget::scrollable::vertical).into()
            }
        }
    }

    fn set_visible(&mut self, files: Vec<File>) {
        self.unwrap_mut().set_visible(files);
    }
}

struct Tags {
    all_tags: Vec<String>,
    available_tags: combo_box::State<String>,
}

type TagsState = LoadedState<Tags>;

pub struct FileList {
    client: Client,
    archive: FileState,
    is_filtering: bool,                   // Track if filtering is in progress
    search_query: String,                 // The search query string
    search_input_id: cosmic::widget::Id,  // Unique ID for focus management
    search_input_is_focussed: bool,       // Flag to indicate search input should be focused
    debounce_counter: u32,                // Counter to track debounce state
    status_filter: Option<ReadingStatus>, // Optional reading status filter
    allow_tags: HashSet<String>,          // Tags that files must have (whitelist)
    deny_tags: HashSet<String>,           // Tags that files must not have (blacklist)
    tags: TagsState,                      // Available tags for selection
    new_allow_tag: String,                // Current input for new allow tag
    new_deny_tag: String,                 // Current input for new deny tag
}

#[derive(Debug, Clone)]
pub enum FileListOutput {
    OpenFileDetails(File),
    ToggleContextPage(ClientSelector),
}

#[derive(Debug, Clone)]
pub enum FileListMessage {
    LoadArchive,
    Loaded(Vec<File>),
    LoadingFailed(String),
    SearchChanged(String),
    ClearSearch,
    FilteringComplete(Vec<File>),
    FocusSearchInput,
    DebounceTimeout(u32, String), // (counter, query) - triggers filtering after delay
    StatusFilterChanged(Option<ReadingStatus>),
    ClearStatusFilter,
    // Tag filtering messages
    LoadAllTags,
    AllTagsLoaded(Result<Vec<String>, String>),
    UpdateNewAllowTag(String),
    AddAllowTag,
    RemoveAllowTag(String),
    UpdateNewDenyTag(String),
    AddDenyTag,
    RemoveDenyTag(String),
    ClearAllTagFilters,
    Out(FileListOutput),
}

impl FileList {
    pub fn selector(&self) -> ClientSelector {
        self.client.selector()
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Attempt to focus the search input using various cosmic framework approaches
    /// This method contains multiple approaches that could work depending on cosmic's API
    fn try_focus_search_input(&self) -> Task<cosmic::Action<FileListMessage>> {
        cosmic::widget::text_input::focus(self.search_input_id.clone())
    }

    /// Start debounce timer - waits for user to stop typing before filtering
    fn start_debounce_timer(
        &self,
        counter: u32,
        query: String,
    ) -> Task<cosmic::Action<FileListMessage>> {
        cosmic::task::future(async move {
            // Wait 250ms for user to stop typing
            tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;
            FileListMessage::DebounceTimeout(counter, query)
        })
    }

    /// Start background filtering task (called after debounce timeout)
    fn start_background_filtering(
        &self,
        query: String,
        status_filter: Option<ReadingStatus>,
        allow_tags: HashSet<String>,
        deny_tags: HashSet<String>,
        all_files: Vec<File>,
    ) -> Task<cosmic::Action<FileListMessage>> {
        cosmic::task::future(async move {
            // Perform filtering in background after debounce timeout
            // This runs only when user has paused typing for 250ms
            let filtered_files = all_files
                .into_iter()
                .filter(|file| {
                    // Filter by search query
                    let matches_search = if query.is_empty() {
                        true
                    } else {
                        let query_lower = query.to_lowercase();
                        let path_lower = file.path.to_lowercase();
                        let tags_lower = file.tags.join(" ").to_lowercase();
                        path_lower.contains(&query_lower) || tags_lower.contains(&query_lower)
                    };

                    // Filter by reading status
                    let matches_status = status_filter.map_or(true, |status| file.status == status);

                    // Filter by allowed tags (file must have ALL allowed tags)
                    let matches_allow_tags = allow_tags.is_empty()
                        || allow_tags.iter().all(|tag| file.tags.contains(tag));

                    // Filter by denied tags (file must have NONE of the denied tags)
                    let matches_deny_tags = deny_tags.is_empty()
                        || !file.tags.iter().any(|tag| deny_tags.contains(tag));

                    matches_search && matches_status && matches_allow_tags && matches_deny_tags
                })
                .collect();

            FileListMessage::FilteringComplete(filtered_files)
        })
    }

    pub fn new(client: Client) -> (Self, Task<cosmic::Action<FileListMessage>>) {
        (
            Self {
                client,
                archive: FileState::default(),
                search_query: String::new(),
                is_filtering: false,
                search_input_id: cosmic::widget::Id::unique(),
                search_input_is_focussed: false,
                debounce_counter: 0,
                status_filter: None,
                allow_tags: HashSet::new(),
                deny_tags: HashSet::new(),
                tags: TagsState::default(),
                new_allow_tag: String::new(),
                new_deny_tag: String::new(),
            },
            Task::batch(vec![
                cosmic::task::message(FileListMessage::LoadArchive),
                cosmic::task::message(FileListMessage::LoadAllTags),
                cosmic::task::message(FileListMessage::FocusSearchInput),
            ]),
        )
    }

    pub fn display_name(&self) -> String {
        self.client.display_name()
    }

    pub fn view(&self) -> Element<FileListMessage> {
        let column = widget::column().spacing(10);

        let header_row = widget::row().align_y(Vertical::Center);

        let header_row = header_row.push(
            widget::button::icon(widget::icon::from_name("open-menu-symbolic"))
                .on_press(FileListMessage::Out(FileListOutput::ToggleContextPage(
                    self.client.selector(),
                )))
                .apply(widget::container)
                .width(Length::Shrink)
                .height(Length::Shrink)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Center),
        );

        let search_input =
            cosmic::widget::text_input(fl!("file-list-search-placeholder"), &self.search_query)
                .id(self.search_input_id.clone())
                .always_active()
                .on_input(FileListMessage::SearchChanged)
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
                .on_press(FileListMessage::ClearSearch)
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
            widget::text(self.client.display_name())
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
                .apply(widget::container)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Horizontal::Left)
                .align_y(Vertical::Top),
        );

        column.into()
    }

    pub fn view_context(&self) -> ContextView<FileListMessage> {
        let mut column = widget::column().spacing(10);

        // Reading Status Filter Section
        let status_section = widget::column()
            .spacing(5)
            .push(widget::text(fl!("file-list-filter-by-status")).size(16))
            .push(
                cosmic::iced::widget::pick_list(
                    [
                        ReadingStatus::Unread,
                        ReadingStatus::Reading,
                        ReadingStatus::Read,
                    ],
                    self.status_filter,
                    |status| FileListMessage::StatusFilterChanged(Some(status)),
                )
                .width(Length::Fill)
                .placeholder(fl!("file-list-all-statuses")),
            )
            .push(
                widget::button::standard(fl!("file-list-clear-filter"))
                    .on_press(FileListMessage::ClearStatusFilter)
                    .width(Length::Fill),
            );

        column = column.push(status_section);

        // Add divider
        column = column.push(cosmic::iced_widget::horizontal_rule(1).width(Length::Fill));

        // Tag Filter Section
        let tag_section = widget::column()
            .spacing(5)
            .push(widget::text(fl!("file-list-filter-by-tags")).size(16));

        let tag_section = self.view_tag_filters(tag_section);

        column = column.push(tag_section);

        ContextView {
            title: fl!("file-list-options-title"),
            content: column.into(),
        }
    }

    fn view_tag_filters<'a>(
        &'a self,
        mut column: widget::Column<'a, FileListMessage>,
    ) -> widget::Column<'a, FileListMessage> {
        // Allow Tags Section
        column = self.view_tag_filter_section(
            column,
            fl!("file-list-allow-tags"),
            &self.allow_tags,
            &self.new_allow_tag,
            FileListMessage::UpdateNewAllowTag,
            FileListMessage::AddAllowTag,
            FileListMessage::RemoveAllowTag,
        );

        // Add spacing
        column = column.push(widget::Space::with_height(Length::Fixed(10.0)));

        // Deny Tags Section
        column = self.view_tag_filter_section(
            column,
            fl!("file-list-deny-tags"),
            &self.deny_tags,
            &self.new_deny_tag,
            FileListMessage::UpdateNewDenyTag,
            FileListMessage::AddDenyTag,
            FileListMessage::RemoveDenyTag,
        );

        // Clear all tag filters button
        if !self.allow_tags.is_empty() || !self.deny_tags.is_empty() {
            column = column.push(
                widget::button::standard(fl!("file-list-clear-all-tag-filters"))
                    .on_press(FileListMessage::ClearAllTagFilters)
                    .width(Length::Fill),
            );
        }

        column
    }

    fn view_tag_filter_section<'a>(
        &'a self,
        mut column: widget::Column<'a, FileListMessage>,
        section_title: String,
        current_tags: &HashSet<String>,
        new_tag_input: &String,
        update_message: fn(String) -> FileListMessage,
        add_message: FileListMessage,
        remove_message_fn: fn(String) -> FileListMessage,
    ) -> widget::Column<'a, FileListMessage> {
        // Section title
        column = column.push(widget::text(section_title));

        // Show current tags
        if !current_tags.is_empty() {
            let tags_row = current_tags
                .iter()
                .fold(widget::row().spacing(5), |row, tag| {
                    row.push(
                        widget::button::standard(format!("✕ {}", tag))
                            .on_press(remove_message_fn(tag.clone())),
                    )
                });
            column = column.push(tags_row);
        }

        // Add tag input
        column = self.view_tag_input(column, new_tag_input, update_message, add_message);

        column
    }

    fn view_tag_input<'a>(
        &'a self,
        column: widget::Column<'a, FileListMessage>,
        new_tag_input: &String,
        update_message: fn(String) -> FileListMessage,
        add_message: FileListMessage,
    ) -> widget::Column<'a, FileListMessage> {
        match &self.tags {
            TagsState::Loaded(Tags {
                all_tags,
                available_tags,
            }) => {
                if all_tags.is_empty() {
                    // No tags exist in the system at all
                    column.push(widget::text(fl!("file-list-no-tags-available")))
                } else {
                    // Check if there are any tags available that aren't already in use
                    let has_available_tags = all_tags
                        .iter()
                        .any(|tag| !self.allow_tags.contains(tag) && !self.deny_tags.contains(tag));

                    if !has_available_tags {
                        // All existing tags are already in use
                        column.push(widget::text(fl!("file-list-all-tags-in-use")))
                    } else {
                        // Show combo box with available tags
                        let combo = combo_box(
                            available_tags,
                            &fl!("file-list-select-tag"),
                            Some(new_tag_input),
                            update_message,
                        )
                        .width(Length::Fill);

                        let add_button = widget::button::standard(fl!("file-list-add-tag"))
                            .on_press(add_message)
                            .width(Length::Shrink);

                        let input_row = widget::row().push(combo).push(add_button).spacing(5);
                        column.push(input_row)
                    }
                }
            }
            TagsState::Loading => column.push(widget::text("Loading tags...")),
            TagsState::Failed(_) => column.push(widget::text("Failed to load tags")),
            TagsState::New => column.push(widget::text("Loading tags...")),
        }
    }

    fn update_available_tags(&mut self, all_tags: Vec<String>) {
        // Filter out tags that are already in either allow or deny lists
        let available_tags: Vec<String> = all_tags
            .iter()
            .filter(|tag| !self.allow_tags.contains(*tag) && !self.deny_tags.contains(*tag))
            .cloned()
            .collect();

        let available_tags_state = combo_box::State::new(available_tags);
        self.tags = TagsState::Loaded(Tags {
            all_tags,
            available_tags: available_tags_state,
        });
    }

    pub fn update(&mut self, message: FileListMessage) -> Task<cosmic::Action<FileListMessage>> {
        tracing::debug!("received: {message:?}");
        match message {
            FileListMessage::LoadArchive => {
                self.archive = FileState::Loading;
                let client = self.client.clone();
                cosmic::task::future(async move {
                    match client.get_files().await {
                        Ok(files) => FileListMessage::Loaded(files),
                        Err(error) => FileListMessage::LoadingFailed(format!("{error}")),
                    }
                })
            }
            FileListMessage::Loaded(files) => {
                // For initial load, use synchronous filtering since it's typically fast
                let files = Files::new(files).filtered_by(
                    &self.search_query,
                    self.status_filter,
                    &self.allow_tags,
                    &self.deny_tags,
                );
                self.archive = FileState::Loaded(files);
                Task::none()
            }
            FileListMessage::LoadingFailed(error) => {
                self.archive = FileState::Failed(error);
                Task::none()
            }
            FileListMessage::SearchChanged(query) => {
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
            FileListMessage::ClearSearch => {
                self.search_query.clear();
                // Reset debounce counter to cancel any pending timers
                self.debounce_counter += 1;

                // Immediately filter to show all files (no debounce needed for clearing)
                if self.archive.is_loaded() && !self.is_filtering {
                    self.is_filtering = true;
                    self.start_background_filtering(
                        String::new(),
                        self.status_filter,
                        self.allow_tags.clone(),
                        self.deny_tags.clone(),
                        self.archive.unwrap().all_files.clone(),
                    )
                } else {
                    Task::none()
                }
            }
            FileListMessage::FilteringComplete(filtered_files) => {
                self.is_filtering = false;
                self.archive.set_visible(filtered_files);
                // Set flag to focus search input after re-render
                self.search_input_is_focussed = true;
                cosmic::task::message(FileListMessage::FocusSearchInput)
            }

            FileListMessage::DebounceTimeout(counter, query) => {
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
                            self.allow_tags.clone(),
                            self.deny_tags.clone(),
                            self.archive.unwrap().all_files.clone(),
                        ),
                        cosmic::task::message(FileListMessage::FocusSearchInput),
                    ])
                } else {
                    // This timeout was superseded by newer typing, ignore it
                    Task::none()
                }
            }
            FileListMessage::FocusSearchInput => {
                self.search_input_is_focussed = false;
                // Use the helper method that contains all the focus approaches to try
                self.try_focus_search_input()
            }
            FileListMessage::StatusFilterChanged(status) => {
                self.status_filter = status;
                // Increment debounce counter to invalidate previous timers
                self.debounce_counter += 1;

                // Immediately filter with new status (no debounce needed for status changes)
                if self.archive.is_loaded() && !self.is_filtering {
                    self.is_filtering = true;
                    self.start_background_filtering(
                        self.search_query.clone(),
                        self.status_filter,
                        self.allow_tags.clone(),
                        self.deny_tags.clone(),
                        self.archive.unwrap().all_files.clone(),
                    )
                } else {
                    Task::none()
                }
            }
            FileListMessage::ClearStatusFilter => {
                self.status_filter = None;
                // Increment debounce counter to invalidate previous timers
                self.debounce_counter += 1;

                // Immediately filter to show all statuses (no debounce needed for clearing)
                if self.archive.is_loaded() && !self.is_filtering {
                    self.is_filtering = true;
                    self.start_background_filtering(
                        self.search_query.clone(),
                        None,
                        self.allow_tags.clone(),
                        self.deny_tags.clone(),
                        self.archive.unwrap().all_files.clone(),
                    )
                } else {
                    Task::none()
                }
            }
            FileListMessage::LoadAllTags => {
                self.tags = TagsState::Loading;
                let client = self.client.clone();
                cosmic::task::future(async move {
                    match client.get_files_tags().await {
                        Ok(tags) => FileListMessage::AllTagsLoaded(Ok(tags)),
                        Err(err) => FileListMessage::AllTagsLoaded(Err(format!("{}", err))),
                    }
                })
            }
            FileListMessage::AllTagsLoaded(result) => {
                match result {
                    Ok(tags) => {
                        tracing::debug!("Loaded {} tags: {:?}", tags.len(), tags);
                        self.update_available_tags(tags);
                    }
                    Err(err) => {
                        tracing::warn!("Failed to load tags: {}", &err);
                        self.tags = TagsState::Failed(err);
                    }
                }
                Task::none()
            }
            FileListMessage::UpdateNewAllowTag(tag) => {
                self.new_allow_tag = tag;
                Task::none()
            }
            FileListMessage::AddAllowTag => {
                if !self.new_allow_tag.is_empty() && !self.allow_tags.contains(&self.new_allow_tag)
                {
                    self.allow_tags.insert(self.new_allow_tag.clone());
                    self.new_allow_tag.clear();

                    // Update available tags to reflect the change
                    if let TagsState::Loaded(Tags { all_tags, .. }) = &self.tags {
                        self.update_available_tags(all_tags.clone());
                    }

                    // Increment debounce counter to invalidate previous timers
                    self.debounce_counter += 1;

                    // Immediately filter with new tag
                    if self.archive.is_loaded() && !self.is_filtering {
                        self.is_filtering = true;
                        self.start_background_filtering(
                            self.search_query.clone(),
                            self.status_filter,
                            self.allow_tags.clone(),
                            self.deny_tags.clone(),
                            self.archive.unwrap().all_files.clone(),
                        )
                    } else {
                        Task::none()
                    }
                } else {
                    Task::none()
                }
            }
            FileListMessage::RemoveAllowTag(tag) => {
                self.allow_tags.remove(&tag);

                // Update available tags to reflect the change
                if let TagsState::Loaded(Tags { all_tags, .. }) = &self.tags {
                    self.update_available_tags(all_tags.clone());
                }

                // Increment debounce counter to invalidate previous timers
                self.debounce_counter += 1;

                // Immediately filter without the removed tag
                if self.archive.is_loaded() && !self.is_filtering {
                    self.is_filtering = true;
                    self.start_background_filtering(
                        self.search_query.clone(),
                        self.status_filter,
                        self.allow_tags.clone(),
                        self.deny_tags.clone(),
                        self.archive.unwrap().all_files.clone(),
                    )
                } else {
                    Task::none()
                }
            }
            FileListMessage::UpdateNewDenyTag(tag) => {
                self.new_deny_tag = tag;
                Task::none()
            }
            FileListMessage::AddDenyTag => {
                if !self.new_deny_tag.is_empty() && !self.deny_tags.contains(&self.new_deny_tag) {
                    self.deny_tags.insert(self.new_deny_tag.clone());
                    self.new_deny_tag.clear();

                    // Update available tags to reflect the change
                    if let TagsState::Loaded(Tags { all_tags, .. }) = &self.tags {
                        self.update_available_tags(all_tags.clone());
                    }

                    // Increment debounce counter to invalidate previous timers
                    self.debounce_counter += 1;

                    // Immediately filter with new tag
                    if self.archive.is_loaded() && !self.is_filtering {
                        self.is_filtering = true;
                        self.start_background_filtering(
                            self.search_query.clone(),
                            self.status_filter,
                            self.allow_tags.clone(),
                            self.deny_tags.clone(),
                            self.archive.unwrap().all_files.clone(),
                        )
                    } else {
                        Task::none()
                    }
                } else {
                    Task::none()
                }
            }
            FileListMessage::RemoveDenyTag(tag) => {
                self.deny_tags.remove(&tag);

                // Update available tags to reflect the change
                if let TagsState::Loaded(Tags { all_tags, .. }) = &self.tags {
                    self.update_available_tags(all_tags.clone());
                }

                // Increment debounce counter to invalidate previous timers
                self.debounce_counter += 1;

                // Immediately filter without the removed tag
                if self.archive.is_loaded() && !self.is_filtering {
                    self.is_filtering = true;
                    self.start_background_filtering(
                        self.search_query.clone(),
                        self.status_filter,
                        self.allow_tags.clone(),
                        self.deny_tags.clone(),
                        self.archive.unwrap().all_files.clone(),
                    )
                } else {
                    Task::none()
                }
            }
            FileListMessage::ClearAllTagFilters => {
                self.allow_tags.clear();
                self.deny_tags.clear();

                // Update available tags to reflect the change
                if let TagsState::Loaded(Tags { all_tags, .. }) = &self.tags {
                    self.update_available_tags(all_tags.clone());
                }

                // Increment debounce counter to invalidate previous timers
                self.debounce_counter += 1;

                // Immediately filter without any tag filters
                if self.archive.is_loaded() && !self.is_filtering {
                    self.is_filtering = true;
                    self.start_background_filtering(
                        self.search_query.clone(),
                        self.status_filter,
                        HashSet::new(),
                        HashSet::new(),
                        self.archive.unwrap().all_files.clone(),
                    )
                } else {
                    Task::none()
                }
            }
            FileListMessage::Out(_) => {
                panic!("should be handled by the parent component")
            }
        }
    }
}

fn view_file<'a>(file: &'a File) -> Element<'a, FileListMessage> {
    display_path(&file.path)
        .apply(cosmic::iced_widget::button)
        .on_press(FileListMessage::Out(FileListOutput::OpenFileDetails(
            file.clone(),
        )))
        .into()
}

fn display_path<'a>(path: &'a str) -> Element<'a, FileListMessage> {
    let path: &Path = path.as_ref();
    let directory = format!("{}", path.parent().unwrap().display());
    let filename = path.file_name().unwrap();
    cosmic::iced_widget::column![
        widget::text(format!("{}", filename.to_string_lossy())),
        widget::text(directory).size(11),
    ]
    .spacing(5)
    .apply(widget::container)
    .width(Length::Fill)
    .into()
}
