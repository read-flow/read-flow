// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashSet;

use archive_organizer::api::File;
use archive_organizer::api::FileDataSource;
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

use crate::app::ContextView;
use crate::client::Client;
use crate::client::ClientSelector;
use crate::component::files::FileState;
use crate::component::files::FilesComponent;
use crate::component::files::FilesMessage;
use crate::component::files::FilesOutput;
use crate::component::pagination::PaginationMessage;
use crate::component::tag_filter::TagFilter;
use crate::component::tag_filter::TagFilterMessage;
use crate::component::tag_filter::TagFilterOutput;
use crate::cosmic_ext::ActionExt;
use crate::fl;
use crate::state::filtered::Filtered;

pub struct FileList {
    client: Client,
    archive: FilesComponent,
    is_filtering: bool,                   // Track if filtering is in progress
    search_query: String,                 // The search query string
    search_input_id: widget::Id,          // Unique ID for focus management
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
    RefreshFile(File),
    SearchChanged(String),
    ClearSearch,
    FilteringComplete(Vec<usize>),
    FocusSearchInput,
    DebounceTimeout(u32, String), // (counter, query) - triggers filtering after delay
    StatusFilterChanged(Option<ReadingStatus>),
    ClearStatusFilter,
    TagFilter(TagFilterMessage),
    FilesComponent(FilesMessage),
    Out(FileListOutput),
}

impl From<TagFilterMessage> for FileListMessage {
    fn from(value: TagFilterMessage) -> Self {
        Self::TagFilter(value)
    }
}

impl From<FilesMessage> for FileListMessage {
    fn from(value: FilesMessage) -> Self {
        Self::FilesComponent(value)
    }
}

impl FileList {
    pub fn selector(&self) -> ClientSelector {
        self.client.selector()
    }

    pub fn client(&self) -> &Client {
        &self.client
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
                .iter()
                .enumerate()
                .filter_map(|(index, file)| {
                    filter_file(&query, status_filter, &allow_tags, &deny_tags, &file)
                        .then_some(index)
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
                archive: FilesComponent::default(),
                search_query: String::new(),
                is_filtering: false,
                search_input_id: widget::Id::unique(),
                debounce_counter: 0,
                status_filter: None,
                tag_filter,
            },
            Task::batch(vec![
                tag_filter_init.map(ActionExt::map_into),
                task::message(FileListMessage::LoadArchive),
                task::message(FileListMessage::FocusSearchInput),
            ]),
        )
    }

    pub fn display_name(&self) -> String {
        self.client.display_name()
    }

    pub fn view(&self) -> Element<'_, FileListMessage> {
        let cosmic_theme::Spacing {
            space_xxs, space_s, ..
        } = theme::active().cosmic().spacing;

        let column = widget::column().spacing(space_xxs);

        let header_row = widget::row().align_y(Vertical::Center).spacing(space_s);

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
                .map(Into::into)
                .apply(widget::container)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Horizontal::Left)
                .align_y(Vertical::Top),
        );

        column.into()
    }

    pub fn view_context(&self) -> ContextView<'_, FileListMessage> {
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
                    |status| FileListMessage::StatusFilterChanged(Some(status)),
                )
                .width(Length::Fill)
                .placeholder(fl!("file-list-all-statuses")),
            )
            .add_maybe(self.status_filter.map(|_| {
                widget::button::text(fl!("file-list-clear-filter"))
                    .on_press(FileListMessage::ClearStatusFilter)
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

    pub fn update(&mut self, message: FileListMessage) -> Task<Action<FileListMessage>> {
        tracing::debug!("received: {message:?}");
        match message {
            FileListMessage::LoadArchive => {
                self.archive.files = FileState::Loading;
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
                let mut files = Filtered::new(files);
                files.filter(|file| {
                    filter_file(
                        &self.search_query,
                        self.status_filter,
                        &self.tag_filter.allow_tags,
                        &self.tag_filter.deny_tags,
                        &file,
                    )
                });

                let collection_size = files.filtered_len();
                self.archive.files = FileState::Loaded(files);
                task::message(FileListMessage::FilesComponent(FilesMessage::Pagination(
                    PaginationMessage::SetCollectionSize(collection_size),
                )))
            }
            FileListMessage::LoadingFailed(error) => {
                self.archive.files = FileState::Failed(error);
                Task::none()
            }
            FileListMessage::RefreshFile(file) => {
                self.archive
                    .files
                    .unwrap_mut()
                    .update_item(move |old_file| old_file.id == file.id, file);
                Task::none()
            }
            FileListMessage::SearchChanged(query) => {
                self.search_query = query.clone();
                // Increment debounce counter to invalidate previous timers
                self.debounce_counter += 1;

                // Only start debounce timer if files have been loaded
                if self.archive.files.is_loaded() {
                    self.start_debounce_timer(self.debounce_counter, query)
                } else {
                    Task::none()
                }
            }
            FileListMessage::ClearSearch => {
                self.search_query.clear();
                // Immediately filter to show all files (no debounce needed for clearing)
                self.filter_now()
            }
            FileListMessage::FilteringComplete(filtered_files) => {
                let collection_size = filtered_files.len();
                self.is_filtering = false;
                self.archive.set_filtered_indices(filtered_files);
                task::message(FileListMessage::FilesComponent(FilesMessage::Pagination(
                    PaginationMessage::SetCollectionSize(collection_size),
                )))
            }
            FileListMessage::DebounceTimeout(counter, query) => {
                // Only proceed if this timeout matches the current counter (not superseded by newer typing)
                if self.archive.files.is_loaded()
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
                            self.archive.files.unwrap().unfiltered().to_vec(),
                        ),
                        task::message(FileListMessage::FocusSearchInput),
                    ])
                } else {
                    // This timeout was superseded by newer typing, ignore it
                    Task::none()
                }
            }
            FileListMessage::FocusSearchInput => {
                widget::text_input::focus(self.search_input_id.clone())
            }
            FileListMessage::StatusFilterChanged(status) => {
                self.status_filter = status;
                // Immediately filter with new status (no debounce needed for status changes)
                self.filter_now()
            }
            FileListMessage::ClearStatusFilter => {
                self.status_filter = None;
                // Immediately filter to show all statuses (no debounce needed for clearing)
                self.filter_now()
            }
            FileListMessage::TagFilter(msg) => match msg {
                TagFilterMessage::Out(msg) => match msg {
                    TagFilterOutput::TagFiltersUpdated => {
                        // Immediately filter to show all statuses (no debounce needed for tag filter changes)
                        self.filter_now()
                    }
                },
                msg => self.tag_filter.update(msg).map(ActionExt::map_into),
            },
            FileListMessage::FilesComponent(msg) => match msg {
                FilesMessage::Out(msg) => match msg {
                    FilesOutput::FileClicked(file) => cosmic::task::message(FileListMessage::Out(
                        FileListOutput::OpenFileDetails(file),
                    )),
                },
                msg => self.archive.update(msg).map(ActionExt::map_into),
            },
            FileListMessage::Out(_) => {
                panic!("should be handled by the parent component")
            }
        }
    }

    fn filter_now(&mut self) -> Task<Action<FileListMessage>> {
        // Increment debounce counter to invalidate previous timers
        self.debounce_counter += 1;

        if self.archive.files.is_loaded() && !self.is_filtering {
            self.is_filtering = true;
            self.start_background_filtering(
                self.search_query.clone(),
                self.status_filter,
                self.tag_filter.allow_tags.clone(),
                self.tag_filter.deny_tags.clone(),
                self.archive.files.unwrap().unfiltered().to_vec(),
            )
        } else {
            Task::none()
        }
    }
}

fn filter_file(
    search_query: &str,
    status_filter: Option<ReadingStatus>,
    allow_tags: &HashSet<String>,
    deny_tags: &HashSet<String>,
    file: &&File,
) -> bool {
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
    let matches_status = status_filter.is_none_or(|status| file.status == status);

    // Filter by allowed tags (file must have ALL allowed tags)
    let matches_allow_tags =
        allow_tags.is_empty() || allow_tags.iter().all(|tag| file.tags.contains(tag));

    // Filter by denied tags (file must have NONE of the denied tags)
    let matches_deny_tags =
        deny_tags.is_empty() || !file.tags.iter().any(|tag| deny_tags.contains(tag));

    matches_search && matches_status && matches_allow_tags && matches_deny_tags
}
