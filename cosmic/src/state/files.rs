// SPDX-License-Identifier: GPL-3.0-or-later

use crate::fl;
use crate::page::file_list::{FileListMessage, FileListOutput};
use crate::state::LoadedState;
use archive_organizer::api::{File, ReadingStatus};
use cosmic::iced::Length;
use cosmic::iced_widget::list::Content;
use cosmic::widget;
use cosmic::{Apply, Element};
use std::collections::HashSet;
use std::path::Path;

pub struct Files {
    all_files: Vec<File>,
    visible_files: Content<File>,
}

impl Files {
    pub fn new(files: Vec<File>) -> Self {
        Self {
            all_files: files.clone(),
            visible_files: Content::with_items(files),
        }
    }

    pub fn set_visible(&mut self, files: Vec<File>) {
        self.visible_files = Content::with_items(files);
    }

    pub fn all_files(&self) -> Vec<File> {
        self.all_files.clone()
    }

    /// Filter files based on the search query, reading status, and tags (synchronous version for initial load only)
    pub fn filtered_by(
        mut self,
        search_query: &str,
        status_filter: Option<ReadingStatus>,
        allow_tags: &HashSet<String>,
        deny_tags: &HashSet<String>,
    ) -> Self {
        let filtered_files = self
            .all_files
            .iter()
            .filter(|file| filter_file(search_query, status_filter, allow_tags, deny_tags, file))
            .cloned()
            .collect();
        self.set_visible(filtered_files);
        self
    }
}

pub fn filter_file(
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

pub type FileState = LoadedState<Files>;

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

    pub fn set_visible(&mut self, files: Vec<File>) {
        self.unwrap_mut().set_visible(files);
    }
}

fn view_file<'a>(file: &'a File) -> Element<'a, FileListMessage> {
    display_path(&file.path)
        .apply(cosmic::iced_widget::button)
        .on_press(FileListMessage::Out(FileListOutput::OpenFileDetails(
            // TODO: should be output message of component.
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
