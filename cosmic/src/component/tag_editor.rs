// SPDX-License-Identifier: GPL-3.0-or-later

use std::fmt::Display;

use archive_organizer::Builder;
use cosmic::Action;
use cosmic::Apply;
use cosmic::Element;
use cosmic::Task;
use cosmic::cosmic_theme;
use cosmic::iced::Length;
use cosmic::iced::alignment::Vertical;
use cosmic::iced::widget::combo_box;
use cosmic::iced_widget::horizontal_rule;
use cosmic::task;
use cosmic::theme;
use cosmic::widget;
use cosmic::widget::Column;
use cosmic::widget::Id;
use cosmic::widget::Row;
use cosmic::widget::text;
use provider::r#async::Provider;

use crate::ICON_SIZE;
use crate::fl;
use crate::state::tags::Tags;
use crate::state::tags::TagsState;

pub enum Orientation {
    Vertical,
    Horizontal,
}

/// Tag editor component for selecting, adding, and removing tags
pub struct TagEditor<P> {
    /// Available tags provider
    pub(super) tags_provider: P,
    /// Orientation of the view
    orientation: Orientation,
    /// Currently selected tags
    selected_tags: Vec<String>,
    /// Tags state for combo box
    tags: TagsState,
    /// Currently selected tag in combo box
    combo_selection: String,
    /// Entered tag in text input
    entered_tag: String,
    /// Placeholder text for combo box
    select_placeholder: String,
    /// Placeholder text for text input
    enter_placeholder: String,
    /// Text for empty state
    empty_text: String,
    /// Tooltip for remove button
    remove_tooltip: String,
    /// Input focus ID
    input_id: Id,
}

#[derive(Debug, Clone)]
pub enum TagEditorOutput {
    /// Tags have been updated (full list of current tags)
    TagsUpdated(Vec<String>),
    /// A tag was added
    TagAdded(String),
    /// A tag was removed
    TagRemoved(String),
}

#[derive(Debug, Clone)]
pub enum TagEditorMessage {
    /// Load all tags from fetcher
    LoadAllTags,
    /// All tags loaded
    AllTagsLoaded(Result<Vec<String>, String>),
    /// Update selected tag in combo box
    UpdateComboSelection(String),
    /// Add selected tag from combo box
    AddSelectedTag,
    /// Update entered tag in text input
    UpdateEnteredTag(String),
    /// Add entered tag from text input
    AddEnteredTag(String),
    /// Remove a tag
    RemoveTag(String),
    /// Set the selected tags (for external updates)
    SetTags(Vec<String>),
    /// Focus the text input
    FocusInput,
    /// Output message (for parent component)
    Out(TagEditorOutput),
}

impl<P, E> TagEditor<P>
where
    P: Provider<Vec<String>, Error = E> + Clone + 'static,
    E: Display,
{
    pub fn new(
        tags_provider: P,
        initial_tags: Vec<String>,
        orientation: Orientation,
        select_placeholder: String,
        enter_placeholder: String,
        empty_text: String,
        remove_tooltip: String,
    ) -> (Self, Task<Action<TagEditorMessage>>) {
        (
            Self {
                tags_provider,
                selected_tags: initial_tags,
                tags: TagsState::default(),
                orientation,
                combo_selection: String::new(),
                entered_tag: String::new(),
                select_placeholder,
                enter_placeholder,
                empty_text,
                remove_tooltip,
                input_id: Id::unique(),
            },
            task::message(TagEditorMessage::LoadAllTags),
        )
    }

    /// View the tag editor
    pub fn view(&self) -> Element<'_, TagEditorMessage> {
        let cosmic_theme::Spacing {
            space_xs, space_s, ..
        } = theme::active().cosmic().spacing;

        if matches!(self.orientation, Orientation::Vertical) {
            Column::new()
                .spacing(space_s)
                .height(Length::Shrink)
                .width(Length::Shrink)
                .push(self.view_current_tags(space_xs))
                .push(horizontal_rule(1))
                .push(self.view_tags_form(space_xs, space_s))
                .into()
        } else {
            Row::new()
                .spacing(space_s)
                .height(Length::Shrink)
                .width(Length::Shrink)
                .push(
                    self.view_current_tags(space_xs)
                        .apply(widget::container)
                        .width(Length::FillPortion(1)),
                )
                .push(
                    self.view_tags_form(space_xs, space_s)
                        .apply(widget::container)
                        .width(Length::FillPortion(1)),
                )
                .into()
        }
    }

    fn view_current_tags(&self, space_xs: u16) -> Element<'_, TagEditorMessage> {
        // Show existing tags
        if self.selected_tags.is_empty() {
            text(&self.empty_text).into()
        } else {
            // Create a flow container for the tags
            // let mut tag_row = FlexRow::new().spacing(space_xs).width(Length::Fill);
            let tag_row = self
                .selected_tags
                .iter()
                .fold(vec![], |mut acc, tag| {
                    let tag_button = widget::button::text(tag)
                        .trailing_icon(widget::icon::from_name("edit-delete-symbolic"))
                        .on_press(TagEditorMessage::RemoveTag(tag.clone()))
                        .tooltip(&self.remove_tooltip);

                    acc.push(tag_button.into());
                    acc
                })
                .apply(widget::Row::with_children)
                .spacing(space_xs)
                .width(Length::Fill);
            tag_row.into()
        }
    }

    fn view_tags_form(&self, space_xs: u16, space_s: u16) -> Element<'_, TagEditorMessage> {
        match &self.tags {
            TagsState::Loaded(Tags { available_tags, .. }) => {
                let mut column = Column::new();

                // Add combo box for tag selection
                let combo = combo_box(
                    available_tags,
                    &self.select_placeholder,
                    Some(&self.combo_selection),
                    TagEditorMessage::UpdateComboSelection,
                )
                .width(Length::Fill);

                let add_button = widget::button::standard(fl!("tag-editor-add"))
                    .apply_if(!self.combo_selection.is_empty(), |button| {
                        button
                            .on_press(TagEditorMessage::AddSelectedTag)
                            .class(widget::button::ButtonClass::Suggested)
                    })
                    .width(Length::Shrink);

                let input_row = Row::new()
                    .push(combo)
                    .push(add_button)
                    .spacing(space_s)
                    .align_y(Vertical::Center);

                column = column.push(input_row);

                // Text input for entering new tags
                let input = widget::text_input(&self.enter_placeholder, &self.entered_tag)
                    .id(self.input_id.clone())
                    .on_input(TagEditorMessage::UpdateEnteredTag)
                    .on_submit(TagEditorMessage::AddEnteredTag)
                    .width(Length::Fill);

                let input_row = Row::new()
                    .push(input)
                    .spacing(space_s)
                    .align_y(Vertical::Center);

                column.push(input_row).into()
            }
            TagsState::Loading => Row::new()
                .spacing(space_xs)
                .align_y(Vertical::Center)
                .push(
                    widget::icon::from_name("content-loading-symbolic")
                        .size(ICON_SIZE)
                        .icon(),
                )
                .push(text(fl!("tag-editor-loading-tags")))
                .into(),

            _ => text(fl!("settings-failed-to-load-tags")).into(),
        }
    }

    /// Update available tags by filtering out already selected tags
    fn update_available_tags(&mut self) {
        if let TagsState::Loaded(Tags { all_tags, .. }) = &self.tags {
            let available: Vec<String> = all_tags
                .iter()
                .filter(|tag| !self.selected_tags.contains(tag))
                .cloned()
                .collect();
            self.tags = TagsState::Loaded(Tags {
                all_tags: all_tags.clone(),
                available_tags: combo_box::State::new(available),
            });
        }
    }

    /// Add a tag and notify parent
    fn add_tag(&mut self, tag: String) -> Task<Action<TagEditorMessage>> {
        let tag = tag.trim().to_string();
        if !tag.is_empty() && !self.selected_tags.contains(&tag) {
            self.selected_tags.push(tag.clone());
            self.selected_tags.sort();
            self.update_available_tags();
            task::batch(vec![
                task::message::<TagEditorMessage, TagEditorMessage>(TagEditorMessage::Out(
                    TagEditorOutput::TagAdded(tag),
                )),
                task::message(TagEditorMessage::Out(TagEditorOutput::TagsUpdated(
                    self.selected_tags.clone(),
                ))),
            ])
        } else {
            Task::none()
        }
    }

    pub fn update(&mut self, message: TagEditorMessage) -> Task<Action<TagEditorMessage>> {
        tracing::debug!("TagEditor received: {message:?}");
        match message {
            TagEditorMessage::LoadAllTags => {
                self.tags = TagsState::Loading;
                let tags_provider = self.tags_provider.clone();
                task::future(async move {
                    TagEditorMessage::AllTagsLoaded(
                        tags_provider.provide().await.map_err(|e| format!("{e}")),
                    )
                })
            }
            TagEditorMessage::AllTagsLoaded(result) => {
                match result {
                    Ok(tags) => {
                        // Filter out already selected tags
                        let available: Vec<String> = tags
                            .iter()
                            .filter(|tag| !self.selected_tags.contains(tag))
                            .cloned()
                            .collect();
                        self.tags = TagsState::Loaded(Tags {
                            all_tags: tags,
                            available_tags: combo_box::State::new(available),
                        });
                    }
                    Err(err) => {
                        tracing::warn!("Failed to load tags: {}", &err);
                        self.tags = TagsState::Failed(err);
                    }
                }
                Task::none()
            }
            TagEditorMessage::UpdateComboSelection(tag) => {
                self.combo_selection = tag;
                Task::none()
            }
            TagEditorMessage::AddSelectedTag => {
                if self.combo_selection.trim().is_empty() {
                    return Task::none();
                }

                let tag = std::mem::take(&mut self.combo_selection);
                self.add_tag(tag)
            }
            TagEditorMessage::UpdateEnteredTag(tag) => {
                self.entered_tag = tag;
                task::message(TagEditorMessage::FocusInput)
            }
            TagEditorMessage::AddEnteredTag(tag) => {
                if tag.trim().is_empty() {
                    return Task::none();
                }
                self.entered_tag.clear();
                self.add_tag(tag)
            }
            TagEditorMessage::RemoveTag(tag) => {
                self.selected_tags.retain(|t| t != &tag);
                self.update_available_tags();
                task::batch(vec![
                    task::message::<TagEditorMessage, TagEditorMessage>(TagEditorMessage::Out(
                        TagEditorOutput::TagRemoved(tag),
                    )),
                    task::message(TagEditorMessage::Out(TagEditorOutput::TagsUpdated(
                        self.selected_tags.clone(),
                    ))),
                ])
            }
            TagEditorMessage::SetTags(tags) => {
                self.selected_tags = tags;
                self.update_available_tags();
                Task::none()
            }
            TagEditorMessage::FocusInput => widget::text_input::focus(self.input_id.clone()),
            TagEditorMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent component")
            }
        }
    }
}
