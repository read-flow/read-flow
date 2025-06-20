// SPDX-License-Identifier: GPL-3.0-or-later

use crate::client::Client;
use crate::fl;
use crate::state::tags::{Tags, TagsState};
use archive_organizer::api::FileDataSource;
use cosmic::iced::Length;
use cosmic::iced::widget::combo_box;
use cosmic::widget;
use cosmic::{Element, Task};
use std::collections::HashSet;

pub struct TagFilter {
    client: Client,
    pub allow_tags: HashSet<String>, // Tags that files must have (whitelist)
    pub deny_tags: HashSet<String>,  // Tags that files must not have (blacklist)
    tags: TagsState,                 // Available tags for selection
    new_allow_tag: String,           // Current input for new allow tag
    new_deny_tag: String,            // Current input for new deny tag
}

#[derive(Debug, Clone)]
pub enum TagFilterOutput {
    TagFiltersUpdated,
}

#[derive(Debug, Clone)]
pub enum TagFilterMessage {
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
    Out(TagFilterOutput),
}

impl TagFilter {
    pub fn new(client: Client) -> (Self, Task<cosmic::Action<TagFilterMessage>>) {
        (
            Self {
                client,
                allow_tags: HashSet::new(),
                deny_tags: HashSet::new(),
                tags: TagsState::default(),
                new_allow_tag: String::new(),
                new_deny_tag: String::new(),
            },
            Task::batch(vec![cosmic::task::message(TagFilterMessage::LoadAllTags)]),
        )
    }

    pub fn view(&self) -> Element<TagFilterMessage> {
        // Tag Filter Section
        let mut column = widget::column()
            .spacing(5)
            .push(widget::text(fl!("file-list-filter-by-tags")).size(16));

        // Allow Tags Section
        column = self.view_tag_filter_section(
            column,
            fl!("file-list-allow-tags"),
            &self.allow_tags,
            &self.new_allow_tag,
            TagFilterMessage::UpdateNewAllowTag,
            TagFilterMessage::AddAllowTag,
            TagFilterMessage::RemoveAllowTag,
        );

        // Add spacing
        column = column.push(widget::Space::with_height(Length::Fixed(10.0)));

        // Deny Tags Section
        column = self.view_tag_filter_section(
            column,
            fl!("file-list-deny-tags"),
            &self.deny_tags,
            &self.new_deny_tag,
            TagFilterMessage::UpdateNewDenyTag,
            TagFilterMessage::AddDenyTag,
            TagFilterMessage::RemoveDenyTag,
        );

        // Clear all tag filters button
        if !self.allow_tags.is_empty() || !self.deny_tags.is_empty() {
            column = column.push(
                widget::button::standard(fl!("file-list-clear-all-tag-filters"))
                    .on_press(TagFilterMessage::ClearAllTagFilters)
                    .width(Length::Fill),
            );
        }

        column.into()
    }

    fn view_tag_filter_section<'a>(
        &'a self,
        mut column: widget::Column<'a, TagFilterMessage>,
        section_title: String,
        current_tags: &HashSet<String>,
        new_tag_input: &String,
        update_message: fn(String) -> TagFilterMessage,
        add_message: TagFilterMessage,
        remove_message_fn: fn(String) -> TagFilterMessage,
    ) -> widget::Column<'a, TagFilterMessage> {
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
        column: widget::Column<'a, TagFilterMessage>,
        new_tag_input: &String,
        update_message: fn(String) -> TagFilterMessage,
        add_message: TagFilterMessage,
    ) -> widget::Column<'a, TagFilterMessage> {
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

    pub fn update(&mut self, message: TagFilterMessage) -> Task<cosmic::Action<TagFilterMessage>> {
        tracing::debug!("received: {message:?}");
        match message {
            TagFilterMessage::LoadAllTags => {
                self.tags = TagsState::Loading;
                let client = self.client.clone();
                cosmic::task::future(async move {
                    match client.get_files_tags().await {
                        Ok(tags) => TagFilterMessage::AllTagsLoaded(Ok(tags)),
                        Err(err) => TagFilterMessage::AllTagsLoaded(Err(format!("{}", err))),
                    }
                })
            }
            TagFilterMessage::AllTagsLoaded(result) => {
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
                cosmic::task::message(TagFilterMessage::Out(TagFilterOutput::TagFiltersUpdated))
            }
            TagFilterMessage::UpdateNewAllowTag(tag) => {
                self.new_allow_tag = tag;
                Task::none()
            }
            TagFilterMessage::AddAllowTag => {
                if !self.new_allow_tag.is_empty() && !self.allow_tags.contains(&self.new_allow_tag)
                {
                    self.allow_tags.insert(self.new_allow_tag.clone());
                    self.new_allow_tag.clear();

                    // Update available tags to reflect the change
                    if let TagsState::Loaded(Tags { all_tags, .. }) = &self.tags {
                        self.update_available_tags(all_tags.clone());

                        // Notify parent of update
                        cosmic::task::message(TagFilterMessage::Out(
                            TagFilterOutput::TagFiltersUpdated,
                        ))
                    } else {
                        Task::none()
                    }
                } else {
                    Task::none()
                }
            }
            TagFilterMessage::RemoveAllowTag(tag) => {
                self.allow_tags.remove(&tag);

                // Update available tags to reflect the change
                if let TagsState::Loaded(Tags { all_tags, .. }) = &self.tags {
                    self.update_available_tags(all_tags.clone());

                    // Notify parent of update
                    cosmic::task::message(TagFilterMessage::Out(TagFilterOutput::TagFiltersUpdated))
                } else {
                    Task::none()
                }
            }
            TagFilterMessage::UpdateNewDenyTag(tag) => {
                self.new_deny_tag = tag;
                Task::none()
            }
            TagFilterMessage::AddDenyTag => {
                if !self.new_deny_tag.is_empty() && !self.deny_tags.contains(&self.new_deny_tag) {
                    self.deny_tags.insert(self.new_deny_tag.clone());
                    self.new_deny_tag.clear();

                    // Update available tags to reflect the change
                    if let TagsState::Loaded(Tags { all_tags, .. }) = &self.tags {
                        self.update_available_tags(all_tags.clone());

                        // Notify parent of update
                        cosmic::task::message(TagFilterMessage::Out(
                            TagFilterOutput::TagFiltersUpdated,
                        ))
                    } else {
                        Task::none()
                    }
                } else {
                    Task::none()
                }
            }
            TagFilterMessage::RemoveDenyTag(tag) => {
                self.deny_tags.remove(&tag);

                // Update available tags to reflect the change
                if let TagsState::Loaded(Tags { all_tags, .. }) = &self.tags {
                    self.update_available_tags(all_tags.clone());

                    // Notify parent of update
                    cosmic::task::message(TagFilterMessage::Out(TagFilterOutput::TagFiltersUpdated))
                } else {
                    Task::none()
                }
            }
            TagFilterMessage::ClearAllTagFilters => {
                self.allow_tags.clear();
                self.deny_tags.clear();

                // Update available tags to reflect the change
                if let TagsState::Loaded(Tags { all_tags, .. }) = &self.tags {
                    self.update_available_tags(all_tags.clone());

                    // Notify parent of update
                    cosmic::task::message(TagFilterMessage::Out(TagFilterOutput::TagFiltersUpdated))
                } else {
                    Task::none()
                }
            }
            TagFilterMessage::Out(_) => {
                panic!("should be handled by the parent component")
            }
        }
    }
}
