// SPDX-License-Identifier: GPL-3.0-or-later

use std::fmt::Display;

use cosmic::Action;
use cosmic::Apply;
use cosmic::Element;
use cosmic::Task;
use cosmic::cosmic_theme;
use cosmic::iced::Length;
use cosmic::iced::alignment::Vertical;
use cosmic::iced::widget::rule;
use cosmic::task;
use cosmic::theme;
use cosmic::widget;
use cosmic::widget::Column;
use cosmic::widget::Row;
use cosmic::widget::text;
use provider::r#async::Provider;
use read_flow_core::Builder;
use read_flow_widgets::ComboBox;

use super::provided_state::ProvidedState;
use super::provided_state::ProvidedStateMessage;
use crate::ICON_SIZE;
use crate::cosmic_ext::ActionExt;
use crate::fl;
use crate::state::tags::Tags;
use crate::state::tags::TagsState;

pub enum Orientation {
    Vertical,
    Horizontal,
}

/// Tag editor component for selecting, adding, and removing tags
pub struct TagEditor<P> {
    /// All tags, loaded from provider
    all_tags: ProvidedState<P, Vec<String>>,
    /// Available tags, derived from all_tags via map
    tags: TagsState,
    /// Orientation of the view
    orientation: Orientation,
    /// Currently selected tags
    selected_tags: Vec<String>,
    /// Current value in the combo box (typed or selected)
    tag_value: String,
    /// Whether the combo box overlay is currently open
    combo_focused: bool,
    /// Placeholder text for the combo box
    placeholder: String,
    /// Text for empty state
    empty_text: String,
    /// Tooltip for remove button
    remove_tooltip: String,
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
    /// Tags provider state
    Tags(ProvidedStateMessage<Vec<String>>),
    /// Update current combo box value (typed)
    UpdateTagValue(String),
    /// An option was clicked in the combo box overlay
    SelectOption(String),
    /// The combo box overlay was opened (input gained focus)
    ComboOpened,
    /// The combo box overlay was closed (input lost focus or dismissed)
    ComboClosed,
    /// Add the current combo box value as a tag
    AddTagValue,
    /// Remove a tag
    RemoveTag(String),
    /// Set the selected tags (for external updates)
    SetTags(Vec<String>),
    /// Output message (for parent component)
    Out(TagEditorOutput),
}

impl From<ProvidedStateMessage<Vec<String>>> for TagEditorMessage {
    fn from(value: ProvidedStateMessage<Vec<String>>) -> Self {
        Self::Tags(value)
    }
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
        placeholder: String,
        empty_text: String,
        remove_tooltip: String,
    ) -> (Self, Task<Action<TagEditorMessage>>) {
        let (all_tags, init_task) = ProvidedState::new(tags_provider);
        (
            Self {
                all_tags,
                tags: TagsState::default(),
                selected_tags: initial_tags,
                orientation,
                tag_value: String::new(),
                combo_focused: false,
                placeholder,
                empty_text,
                remove_tooltip,
            },
            init_task.map(ActionExt::map_into),
        )
    }

    pub fn set_provider(&mut self, provider: P) {
        self.all_tags.set_provider(provider);
    }

    pub fn provider_mut(&mut self) -> &mut P {
        self.all_tags.provider_mut()
    }

    /// View the tag editor
    pub fn view(&self) -> Element<'_, TagEditorMessage> {
        let cosmic_theme::Spacing {
            space_xs, space_s, ..
        } = theme::active().cosmic().spacing;

        if matches!(self.orientation, Orientation::Vertical) {
            Column::new()
                .spacing(space_s)
                .width(Length::Fill)
                .push(self.view_current_tags(space_xs))
                .push(rule::horizontal(1))
                .push(self.view_tags_form(space_xs))
                .into()
        } else {
            Row::new()
                .spacing(space_s)
                .width(Length::Fill)
                .push(
                    self.view_current_tags(space_xs)
                        .apply(widget::container)
                        .width(Length::FillPortion(1)),
                )
                .push(
                    self.view_tags_form(space_xs)
                        .apply(widget::container)
                        .width(Length::FillPortion(1)),
                )
                .into()
        }
    }

    fn view_current_tags(&self, space_xs: u16) -> Element<'_, TagEditorMessage> {
        if self.selected_tags.is_empty() {
            text(&self.empty_text).into()
        } else {
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
                .apply(widget::flex_row)
                .spacing(space_xs)
                .width(Length::Fill);
            tag_row.into()
        }
    }

    fn view_tags_form(&self, space_xs: u16) -> Element<'_, TagEditorMessage> {
        match &self.tags {
            TagsState::Loaded(Tags { available_tags, .. }) => {
                let add_button = widget::button::standard(fl!("tag-editor-add"))
                    .apply_if(!self.tag_value.is_empty(), |button| {
                        button
                            .on_press(TagEditorMessage::AddTagValue)
                            .class(widget::button::ButtonClass::Suggested)
                    })
                    .width(Length::Shrink);

                let horizontal_space =
                    widget::space::horizontal().width(Length::Fixed(space_xs.into()));

                let combo = ComboBox::new(
                    available_tags,
                    &self.placeholder,
                    &self.tag_value,
                    TagEditorMessage::UpdateTagValue,
                )
                .width(Length::Fill)
                .on_select(TagEditorMessage::SelectOption)
                .on_open(TagEditorMessage::ComboOpened)
                .on_close(TagEditorMessage::ComboClosed)
                .focused(self.combo_focused)
                .view();

                widget::Row::with_children(vec![combo, horizontal_space.into(), add_button.into()])
                    .width(Length::Fill)
                    .into()
            }
            TagsState::Loading => Row::new()
                .width(Length::Fill)
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
        self.tags = self.all_tags.state.map(|all_tags| {
            let available: Vec<String> = all_tags
                .iter()
                .filter(|tag| !self.selected_tags.contains(tag))
                .cloned()
                .collect();
            Tags {
                all_tags: all_tags.clone(),
                available_tags: available,
            }
        });
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
            TagEditorMessage::Tags(msg) => {
                let task = self.all_tags.update(msg).map(ActionExt::map_into);
                self.update_available_tags();
                task
            }
            TagEditorMessage::UpdateTagValue(value) => {
                self.tag_value = value;
                self.combo_focused = true;
                Task::none()
            }
            TagEditorMessage::SelectOption(text) => {
                self.tag_value = text;
                self.combo_focused = false;
                Task::none()
            }
            TagEditorMessage::ComboOpened => {
                self.combo_focused = true;
                Task::none()
            }
            TagEditorMessage::ComboClosed => {
                self.combo_focused = false;
                Task::none()
            }
            TagEditorMessage::AddTagValue => {
                if self.tag_value.trim().is_empty() {
                    return Task::none();
                }
                let tag = std::mem::take(&mut self.tag_value);
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
            TagEditorMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent component")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use cosmic_golden::golden_test;
    use provider::r#async::Value;

    use super::Orientation;
    use super::TagEditor;
    use super::TagEditorMessage;

    fn all_tags() -> Vec<String> {
        vec![
            "fiction".to_string(),
            "non-fiction".to_string(),
            "programming".to_string(),
            "rust".to_string(),
        ]
    }

    fn make_editor(
        selected: Vec<String>,
        orientation: Orientation,
    ) -> TagEditor<Value<Vec<String>>> {
        let (editor, _) = TagEditor::new(
            Value::new(all_tags()),
            selected,
            orientation,
            "Select a tag".to_string(),
            "No tags".to_string(),
            "Remove".to_string(),
        );
        editor
    }

    #[golden_test(400, 150)]
    fn tag_editor_empty() -> cosmic::Element<'_, TagEditorMessage> {
        let editor = make_editor(vec![], Orientation::Vertical);
        editor.view()
    }

    #[golden_test(400, 150, dark)]
    fn tag_editor_empty_dark() -> cosmic::Element<'_, TagEditorMessage> {
        let editor = make_editor(vec![], Orientation::Vertical);
        editor.view()
    }

    #[golden_test(400, 150)]
    fn tag_editor_with_tags() -> cosmic::Element<'_, TagEditorMessage> {
        let editor = make_editor(
            vec!["fiction".to_string(), "rust".to_string()],
            Orientation::Vertical,
        );
        editor.view()
    }

    #[golden_test(400, 150, dark)]
    fn tag_editor_with_tags_dark() -> cosmic::Element<'_, TagEditorMessage> {
        let editor = make_editor(
            vec!["fiction".to_string(), "rust".to_string()],
            Orientation::Vertical,
        );
        editor.view()
    }

    #[golden_test(600, 150)]
    fn tag_editor_with_tags_horizontal() -> cosmic::Element<'_, TagEditorMessage> {
        let editor = make_editor(
            vec!["fiction".to_string(), "rust".to_string()],
            Orientation::Horizontal,
        );
        editor.view()
    }

    #[golden_test(600, 150, dark)]
    fn tag_editor_with_tags_horizontal_dark() -> cosmic::Element<'_, TagEditorMessage> {
        let editor = make_editor(
            vec!["fiction".to_string(), "rust".to_string()],
            Orientation::Horizontal,
        );
        editor.view()
    }
}
