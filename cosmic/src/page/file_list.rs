// SPDX-License-Identifier: GPL-3.0-or-later

use crate::app::ContextView;
use crate::client::{Client, ClientSelector};
use crate::fl;
use crate::state::LoadedState;
use archive_organizer::api::{File, FileDataSource};
use cosmic::iced::Length;
use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::iced_widget::list::Content;
use cosmic::widget;
use cosmic::{Apply, Element, Task};
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

    /// Filter files based on the search query (synchronous version for initial load only)
    fn filtered_by(mut self, search_query: &str) -> Self {
        // unwraps on self.archive are safe because of check above
        if search_query.is_empty() {
            // noop
        } else {
            let query = search_query.to_lowercase();
            let filtered_files = self
                .all_files
                .iter()
                .filter(|file| {
                    // Search in file path and tags
                    let path_lower = file.path.to_lowercase();
                    let tags_lower = file.tags.join(" ").to_lowercase();

                    path_lower.contains(&query) || tags_lower.contains(&query)
                })
                .cloned()
                .collect();
            self.set_visible(filtered_files);
        }
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

pub struct FileList {
    client: Client,
    archive: FileState,
    is_filtering: bool,                  // Track if filtering is in progress
    search_query: String,                // The search query string
    search_input_id: cosmic::widget::Id, // Unique ID for focus management
    search_input_is_focussed: bool,      // Flag to indicate search input should be focused
    debounce_counter: u32,               // Counter to track debounce state
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
        all_files: Vec<File>,
    ) -> Task<cosmic::Action<FileListMessage>> {
        cosmic::task::future(async move {
            // Perform filtering in background after debounce timeout
            // This runs only when user has paused typing for 250ms
            let filtered_files = if query.is_empty() {
                all_files
            } else {
                let query_lower = query.to_lowercase();
                all_files
                    .into_iter()
                    .filter(|file| {
                        // Search in file path and tags
                        let path_lower = file.path.to_lowercase();
                        let tags_lower = file.tags.join(" ").to_lowercase();

                        path_lower.contains(&query_lower) || tags_lower.contains(&query_lower)
                    })
                    .collect()
            };

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
            },
            Task::batch(vec![
                cosmic::task::message(FileListMessage::LoadArchive),
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
        ContextView {
            title: "File List".to_string(),
            content: widget::text("TODO").into(),
        }
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
                let files = Files::new(files).filtered_by(&self.search_query);
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
