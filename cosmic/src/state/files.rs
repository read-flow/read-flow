// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashSet;

use archive_organizer::api::{File, ReadingStatus};

use crate::state::LoadedState;

pub struct Files {
    pub all_files: Vec<File>,
    pub filtered_indices: Vec<usize>,
}

impl Files {
    pub fn new(files: Vec<File>) -> Self {
        Self {
            filtered_indices: files.iter().enumerate().map(|(index, _)| index).collect(),
            all_files: files,
        }
    }

    pub fn set_visible(&mut self, files: Vec<usize>) {
        self.filtered_indices = files;
    }

    pub fn update_file_by_id(&mut self, updated_file: File) {
        if let Some(file) = self
            .all_files
            .iter_mut()
            .find(|file| file.id == updated_file.id)
        {
            *file = updated_file;
        }
    }

    pub fn all_files(&self) -> Vec<File> {
        self.all_files.clone()
    }

    pub fn filtered_files(&self) -> Vec<&File> {
        self.filtered_indices
            .iter()
            .map(|index| &self.all_files[*index])
            .collect()
    }

    /// Filter files based on the search query, reading status, and tags (synchronous version for initial load only)
    pub fn filtered_by(
        mut self,
        search_query: &str,
        status_filter: Option<ReadingStatus>,
        allow_tags: &HashSet<String>,
        deny_tags: &HashSet<String>,
    ) -> Self {
        // Only show current selection
        let filtered_files = self
            .all_files
            .iter()
            .enumerate()
            .filter_map(|(index, file)| {
                filter_file(search_query, status_filter, allow_tags, deny_tags, &file)
                    .then_some(index)
            })
            .collect::<Vec<_>>();

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
