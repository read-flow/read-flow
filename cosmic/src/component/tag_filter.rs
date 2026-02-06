// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashSet;
use std::fmt::Display;

use archive_organizer::Builder;
use cosmic::Element;
use cosmic::Task;
use cosmic::cosmic_theme;
use cosmic::iced::Length;
use cosmic::iced::widget::combo_box;
use cosmic::theme;
use cosmic::widget;
use cosmic::widget::settings;
use provider::r#async::Provider;

use super::provided_state::ProvidedState;
use super::provided_state::ProvidedStateMessage;
use crate::cosmic_ext::ActionExt;
use crate::fl;
use crate::state::tags::Tags;
use crate::state::tags::TagsState;

pub struct TagFilter<P> {
    all_tags: ProvidedState<P, Vec<String>>,
    tags: TagsState,
    pub allow_tags: HashSet<String>, // Tags that files must have (whitelist)
    pub deny_tags: HashSet<String>,  // Tags that files must not have (blacklist)
    new_allow_tag: String,           // Current input for new allow tag
    new_deny_tag: String,            // Current input for new deny tag
}

#[derive(Debug, Clone)]
pub enum TagFilterOutput {
    TagFiltersUpdated,
}

#[derive(Debug, Clone)]
pub enum TagFilterMessage {
    // Tag loading
    Tags(ProvidedStateMessage<Vec<String>>),
    // Tag filtering messages
    UpdateNewAllowTag(String),
    AddAllowTag,
    RemoveAllowTag(String),
    UpdateNewDenyTag(String),
    AddDenyTag,
    RemoveDenyTag(String),
    ClearAllTagFilters,
    Out(TagFilterOutput),
}

impl From<ProvidedStateMessage<Vec<String>>> for TagFilterMessage {
    fn from(value: ProvidedStateMessage<Vec<String>>) -> Self {
        Self::Tags(value)
    }
}

impl<P, E> TagFilter<P>
where
    P: Provider<Vec<String>, Error = E> + Clone + 'static,
    E: Display,
{
    pub fn new(tags_provider: P) -> (Self, Task<cosmic::Action<TagFilterMessage>>) {
        let (all_tags, init_task) = ProvidedState::new(tags_provider);
        (
            Self {
                all_tags,
                tags: TagsState::default(),
                allow_tags: HashSet::new(),
                deny_tags: HashSet::new(),
                new_allow_tag: String::new(),
                new_deny_tag: String::new(),
            },
            init_task.map(ActionExt::map_into),
        )
    }

    pub fn view(&self) -> Element<'_, TagFilterMessage> {
        let mut content = Vec::new();

        // Allow Tags Section
        let allow_section = self.view_tag_filter_section(
            fl!("document-list-allow-tags"),
            &self.allow_tags,
            &self.new_allow_tag,
            TagFilterMessage::UpdateNewAllowTag,
            TagFilterMessage::AddAllowTag,
            TagFilterMessage::RemoveAllowTag,
        );
        content.push(allow_section.into());

        // Deny Tags Section
        let deny_section = self.view_tag_filter_section(
            fl!("document-list-deny-tags"),
            &self.deny_tags,
            &self.new_deny_tag,
            TagFilterMessage::UpdateNewDenyTag,
            TagFilterMessage::AddDenyTag,
            TagFilterMessage::RemoveDenyTag,
        );
        content.push(deny_section.into());

        // Clear all tag filters button
        if !self.allow_tags.is_empty() || !self.deny_tags.is_empty() {
            let clear_section = settings::section().add(
                widget::button::text(fl!("document-list-clear-all-tag-filters"))
                    .on_press(TagFilterMessage::ClearAllTagFilters),
            );
            content.push(clear_section.into());
        }

        settings::view_column(content).into()
    }

    #[allow(clippy::too_many_arguments)]
    fn view_tag_filter_section<'a>(
        &'a self,
        section_title: String,
        current_tags: &HashSet<String>,
        new_tag_input: &String,
        update_message: fn(String) -> TagFilterMessage,
        add_message: TagFilterMessage,
        remove_message_fn: fn(String) -> TagFilterMessage,
    ) -> settings::Section<'a, TagFilterMessage> {
        let cosmic_theme::Spacing { space_xs, .. } = theme::active().cosmic().spacing;

        let mut section = settings::section().title(section_title);

        // Show current tags as removable chips
        if !current_tags.is_empty() {
            let tags: Vec<_> = current_tags
                .iter()
                .map(|tag| {
                    widget::button::text(format!("✕ {tag}"))
                        .on_press(remove_message_fn(tag.clone()))
                        .into()
                })
                .collect();

            let tags_flex = widget::flex_row(tags)
                .row_spacing(space_xs)
                .column_spacing(space_xs);

            section = section.add(tags_flex);
        } else {
            section = section.add(widget::text::caption(fl!("document-list-no-tags-selected")));
        }

        // Add tag input
        section = self.view_tag_input(section, new_tag_input, update_message, add_message);

        section
    }

    fn view_tag_input<'a>(
        &'a self,
        section: settings::Section<'a, TagFilterMessage>,
        new_tag_input: &String,
        update_message: fn(String) -> TagFilterMessage,
        add_message: TagFilterMessage,
    ) -> settings::Section<'a, TagFilterMessage> {
        let cosmic_theme::Spacing { space_xs, .. } = theme::active().cosmic().spacing;
        match &self.tags {
            TagsState::Loaded(Tags {
                all_tags,
                available_tags,
            }) => {
                if all_tags.is_empty() {
                    // No tags exist in the system at all
                    section.add(widget::text::caption(fl!(
                        "document-list-no-tags-available"
                    )))
                } else {
                    // Check if there are any tags available that aren't already in use
                    let has_available_tags = all_tags
                        .iter()
                        .any(|tag| !self.allow_tags.contains(tag) && !self.deny_tags.contains(tag));

                    if !has_available_tags {
                        // All existing tags are already in use
                        section.add(widget::text::caption(fl!("document-list-all-tags-in-use")))
                    } else {
                        // Show combo box with available tags
                        let combo = combo_box(
                            available_tags,
                            &fl!("document-list-select-tag"),
                            Some(new_tag_input),
                            update_message,
                        )
                        .width(Length::Fill);

                        let add_button = widget::button::text(fl!("document-list-add-tag"))
                            .apply_if(!new_tag_input.is_empty(), |button| {
                                button
                                    .on_press(add_message)
                                    .class(widget::button::ButtonClass::Suggested)
                            });

                        let input_row =
                            widget::row().push(combo).push(add_button).spacing(space_xs);
                        section.add(input_row)
                    }
                }
            }
            TagsState::Loading => section.add(widget::text::caption("Loading tags...")),
            TagsState::Failed(_) => section.add(widget::text::caption("Failed to load tags")),
            TagsState::New => section.add(widget::text::caption("Loading tags...")),
        }
    }

    fn update_available_tags(&mut self) {
        self.tags = self.all_tags.state.map(|all_tags| {
            let available_tags: Vec<String> = all_tags
                .iter()
                .filter(|tag| !self.allow_tags.contains(*tag) && !self.deny_tags.contains(*tag))
                .cloned()
                .collect();
            Tags {
                all_tags: all_tags.clone(),
                available_tags: combo_box::State::new(available_tags),
            }
        });
    }

    pub fn update(&mut self, message: TagFilterMessage) -> Task<cosmic::Action<TagFilterMessage>> {
        tracing::debug!("received: {message:?}");
        match message {
            TagFilterMessage::Tags(msg) => {
                let send_notification = !matches!(msg, ProvidedStateMessage::Load);
                let task = self.all_tags.update(msg).map(ActionExt::map_into);
                self.update_available_tags();
                if send_notification {
                    Task::batch(vec![
                        task,
                        cosmic::task::message(TagFilterMessage::Out(
                            TagFilterOutput::TagFiltersUpdated,
                        )),
                    ])
                } else {
                    task
                }
            }
            TagFilterMessage::UpdateNewAllowTag(tag) => {
                self.new_allow_tag = tag;
                Task::none()
            }
            TagFilterMessage::AddAllowTag => {
                if !self.new_allow_tag.is_empty() && !self.allow_tags.contains(&self.new_allow_tag)
                {
                    self.allow_tags
                        .insert(std::mem::take(&mut self.new_allow_tag));
                    self.update_available_tags();
                    cosmic::task::message(TagFilterMessage::Out(TagFilterOutput::TagFiltersUpdated))
                } else {
                    Task::none()
                }
            }
            TagFilterMessage::RemoveAllowTag(tag) => {
                self.allow_tags.remove(&tag);
                self.update_available_tags();
                cosmic::task::message(TagFilterMessage::Out(TagFilterOutput::TagFiltersUpdated))
            }
            TagFilterMessage::UpdateNewDenyTag(tag) => {
                self.new_deny_tag = tag;
                Task::none()
            }
            TagFilterMessage::AddDenyTag => {
                if !self.new_deny_tag.is_empty() && !self.deny_tags.contains(&self.new_deny_tag) {
                    self.deny_tags
                        .insert(std::mem::take(&mut self.new_deny_tag));
                    self.update_available_tags();
                    cosmic::task::message(TagFilterMessage::Out(TagFilterOutput::TagFiltersUpdated))
                } else {
                    Task::none()
                }
            }
            TagFilterMessage::RemoveDenyTag(tag) => {
                self.deny_tags.remove(&tag);
                self.update_available_tags();
                cosmic::task::message(TagFilterMessage::Out(TagFilterOutput::TagFiltersUpdated))
            }
            TagFilterMessage::ClearAllTagFilters => {
                self.allow_tags.clear();
                self.deny_tags.clear();
                self.update_available_tags();
                cosmic::task::message(TagFilterMessage::Out(TagFilterOutput::TagFiltersUpdated))
            }
            TagFilterMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent component")
            }
        }
    }
}
