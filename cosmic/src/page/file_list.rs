// SPDX-License-Identifier: GPL-3.0-or-later

use crate::app::ContextView;
use crate::client::{Client, ClientSelector};
use crate::component::tag_filter::TagFilterOutput;
use crate::component::tag_filter::{TagFilter, TagFilterMessage};
use crate::cosmic_ext::ActionExt;
use crate::fl;
use crate::state::files::{filter_file, FileState};
use crate::state::files::Files;
use archive_organizer::api::{File, FileDataSource, ReadingStatus};
use cosmic::iced;
use cosmic::iced::Length;
use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::iced_widget;
use cosmic::task;
use cosmic::{Action, widget};
use cosmic::{Apply, Element, Task};
use std::collections::HashSet;

pub struct FileList {
    client: Client,
    archive: FileState,
    is_filtering: bool,                   // Track if filtering is in progress
    search_query: String,                 // The search query string
    search_input_id: widget::Id,          // Unique ID for focus management
    search_input_is_focussed: bool,       // Flag to indicate search input should be focused
    debounce_counter: u32,                // Counter to track debounce state
    status_filter: Option<ReadingStatus>, // Optional reading status filter
    tag_filter: TagFilter,                // Tag Filter component
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
    TagFilter(TagFilterMessage),
    Out(FileListOutput),
}

impl From<TagFilterMessage> for FileListMessage {
    fn from(value: TagFilterMessage) -> Self {
        Self::TagFilter(value)
    }
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
    fn try_focus_search_input(&self) -> Task<Action<FileListMessage>> {
        widget::text_input::focus(self.search_input_id.clone())
    }

    /// Start debounce timer - waits for user to stop typing before filtering
    fn start_debounce_timer(&self, counter: u32, query: String) -> Task<Action<FileListMessage>> {
        task::future(async move {
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
    ) -> Task<Action<FileListMessage>> {
        task::future(async move {
            // Perform filtering in background after debounce timeout
            // This runs only when user has paused typing for 250ms
            let filtered_files = all_files
                .into_iter()
                .filter(|file| {
                    filter_file(&query, status_filter, &allow_tags, &deny_tags, &file)
                })
                .collect();

            FileListMessage::FilteringComplete(filtered_files)
        })
    }

    pub fn new(client: Client) -> (Self, Task<Action<FileListMessage>>) {
        let (tag_filter, tag_filter_init) = TagFilter::new(client.clone());
        (
            Self {
                client,
                archive: FileState::default(),
                search_query: String::new(),
                is_filtering: false,
                search_input_id: widget::Id::unique(),
                search_input_is_focussed: false,
                debounce_counter: 0,
                status_filter: None,
                tag_filter,
            },
            Task::batch(vec![
                tag_filter_init.map(|action| action.map(Into::into)),
                task::message(FileListMessage::LoadArchive),
                task::message(FileListMessage::FocusSearchInput),
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
            widget::text_input(fl!("file-list-search-placeholder"), &self.search_query)
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
                iced::widget::pick_list(
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
        column = column.push(iced_widget::horizontal_rule(1).width(Length::Fill));

        column = column.push(self.tag_filter.view().map(Into::into));

        ContextView {
            title: fl!("file-list-options-title"),
            content: column.into(),
        }
    }

    pub fn update(&mut self, message: FileListMessage) -> Task<Action<FileListMessage>> {
        tracing::debug!("received: {message:?}");
        match message {
            FileListMessage::LoadArchive => {
                self.archive = FileState::Loading;
                let client = self.client.clone();
                task::future(async move {
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
                    &self.tag_filter.allow_tags,
                    &self.tag_filter.deny_tags,
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
                        self.tag_filter.allow_tags.clone(),
                        self.tag_filter.deny_tags.clone(),
                        self.archive.unwrap().all_files(),
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
                task::message(FileListMessage::FocusSearchInput)
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
                            self.tag_filter.allow_tags.clone(),
                            self.tag_filter.deny_tags.clone(),
                            self.archive.unwrap().all_files(),
                        ),
                        task::message(FileListMessage::FocusSearchInput),
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
                        self.tag_filter.allow_tags.clone(),
                        self.tag_filter.deny_tags.clone(),
                        self.archive.unwrap().all_files(),
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
                        self.tag_filter.allow_tags.clone(),
                        self.tag_filter.deny_tags.clone(),
                        self.archive.unwrap().all_files(),
                    )
                } else {
                    Task::none()
                }
            }
            FileListMessage::TagFilter(msg) => match msg {
                TagFilterMessage::Out(msg) => match msg {
                    TagFilterOutput::TagFiltersUpdated => {
                        // Increment debounce counter to invalidate previous timers
                        self.debounce_counter += 1;

                        // Immediately filter to show all statuses (no debounce needed for clearing)
                        if self.archive.is_loaded() && !self.is_filtering {
                            self.is_filtering = true;
                            self.start_background_filtering(
                                self.search_query.clone(),
                                None,
                                self.tag_filter.allow_tags.clone(),
                                self.tag_filter.deny_tags.clone(),
                                self.archive.unwrap().all_files(),
                            )
                        } else {
                            Task::none()
                        }
                    }
                },
                msg => self
                    .tag_filter
                    .update(msg)
                    .map(move |action| action.map(Into::into)),
            },
            FileListMessage::Out(_) => {
                panic!("should be handled by the parent component")
            }
        }
    }
}
