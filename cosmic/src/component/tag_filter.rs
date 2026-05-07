// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashSet;
use std::fmt::Display;

use cosmic::Element;
use cosmic::Task;
use cosmic::cosmic_theme;
use cosmic::iced::Length;
use cosmic::theme;
use cosmic::widget;
use cosmic::widget::settings;
use provider::r#async::Provider;
use read_flow_widgets::ComboBox;

use super::provided_state::ProvidedState;
use super::provided_state::ProvidedStateMessage;
use crate::cosmic_ext::ActionExt;
use crate::fl;
use crate::state::tags::Tags;
use crate::state::tags::TagsState;

enum TagKind {
    Allow,
    Deny,
}

pub struct TagFilter<P> {
    all_tags: ProvidedState<P, Vec<String>>,
    tags: TagsState,
    pub allow_tags: HashSet<String>,
    pub deny_tags: HashSet<String>,
    new_allow_tag: String,
    new_deny_tag: String,
    allow_focused: bool,
    deny_focused: bool,
}

#[derive(Debug, Clone)]
pub enum TagFilterOutput {
    TagFiltersUpdated,
}

#[derive(Debug, Clone)]
pub enum TagFilterMessage {
    Tags(ProvidedStateMessage<Vec<String>>),
    UpdateNewAllowTag(String),
    SelectAllowTag(String),
    AllowComboOpened,
    AllowComboClosed,
    RemoveAllowTag(String),
    UpdateNewDenyTag(String),
    SelectDenyTag(String),
    DenyComboOpened,
    DenyComboClosed,
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
                allow_focused: false,
                deny_focused: false,
            },
            init_task.map(ActionExt::map_into),
        )
    }

    pub fn view(&self) -> Element<'_, TagFilterMessage> {
        let mut content = Vec::new();

        // Allow Tags Section
        let allow_section = self.view_tag_filter_section(TagKind::Allow);
        content.push(allow_section.into());

        // Deny Tags Section
        let deny_section = self.view_tag_filter_section(TagKind::Deny);
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

    fn view_tag_filter_section<'a>(
        &'a self,
        kind: TagKind,
    ) -> settings::Section<'a, TagFilterMessage> {
        let (
            section_title,
            current_tags,
            new_tag_input,
            update_message,
            select_message,
            remove_message_fn,
            focused,
            open_message,
            close_message,
        ) = match kind {
            TagKind::Allow => (
                fl!("document-list-allow-tags"),
                &self.allow_tags,
                self.new_allow_tag.as_str(),
                TagFilterMessage::UpdateNewAllowTag as fn(String) -> TagFilterMessage,
                TagFilterMessage::SelectAllowTag as fn(String) -> TagFilterMessage,
                TagFilterMessage::RemoveAllowTag as fn(String) -> TagFilterMessage,
                self.allow_focused,
                TagFilterMessage::AllowComboOpened,
                TagFilterMessage::AllowComboClosed,
            ),
            TagKind::Deny => (
                fl!("document-list-deny-tags"),
                &self.deny_tags,
                self.new_deny_tag.as_str(),
                TagFilterMessage::UpdateNewDenyTag as fn(String) -> TagFilterMessage,
                TagFilterMessage::SelectDenyTag as fn(String) -> TagFilterMessage,
                TagFilterMessage::RemoveDenyTag as fn(String) -> TagFilterMessage,
                self.deny_focused,
                TagFilterMessage::DenyComboOpened,
                TagFilterMessage::DenyComboClosed,
            ),
        };

        let cosmic_theme::Spacing { space_xs, .. } = theme::active().cosmic().spacing;

        let mut section = settings::section().title(section_title);

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

        match &self.tags {
            TagsState::Loaded(Tags {
                all_tags,
                available_tags,
            }) => {
                if all_tags.is_empty() {
                    section.add(widget::text::caption(fl!("document-list-no-tags-available")))
                } else if available_tags.is_empty() {
                    section.add(widget::text::caption(fl!("document-list-all-tags-in-use")))
                } else {
                    let combo = ComboBox::new(
                        available_tags,
                        fl!("document-list-select-tag"),
                        new_tag_input,
                        update_message,
                    )
                    .on_select(select_message)
                    .on_open(open_message)
                    .on_close(close_message)
                    .focused(focused)
                    .width(Length::Fill)
                    .view();

                    section.add(combo)
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
                available_tags,
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
                self.allow_focused = true;
                Task::none()
            }
            TagFilterMessage::SelectAllowTag(tag) => {
                if !self.allow_tags.contains(&tag) {
                    self.allow_tags.insert(tag);
                    self.new_allow_tag.clear();
                    self.update_available_tags();
                    cosmic::task::message(TagFilterMessage::Out(TagFilterOutput::TagFiltersUpdated))
                } else {
                    Task::none()
                }
            }
            TagFilterMessage::AllowComboOpened => {
                self.allow_focused = true;
                Task::none()
            }
            TagFilterMessage::AllowComboClosed => {
                self.allow_focused = false;
                Task::none()
            }
            TagFilterMessage::RemoveAllowTag(tag) => {
                self.allow_tags.remove(&tag);
                self.update_available_tags();
                cosmic::task::message(TagFilterMessage::Out(TagFilterOutput::TagFiltersUpdated))
            }
            TagFilterMessage::UpdateNewDenyTag(tag) => {
                self.new_deny_tag = tag;
                self.deny_focused = true;
                Task::none()
            }
            TagFilterMessage::SelectDenyTag(tag) => {
                if !self.deny_tags.contains(&tag) {
                    self.deny_tags.insert(tag);
                    self.new_deny_tag.clear();
                    self.update_available_tags();
                    cosmic::task::message(TagFilterMessage::Out(TagFilterOutput::TagFiltersUpdated))
                } else {
                    Task::none()
                }
            }
            TagFilterMessage::DenyComboOpened => {
                self.deny_focused = true;
                Task::none()
            }
            TagFilterMessage::DenyComboClosed => {
                self.deny_focused = false;
                Task::none()
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

#[cfg(test)]
mod tests {
    use cosmic_golden::golden_test;
    use provider::r#async::Value;

    use super::TagFilter;
    use super::TagFilterMessage;

    fn tags() -> Vec<String> {
        vec![
            "fiction".to_string(),
            "non-fiction".to_string(),
            "rust".to_string(),
            "unread".to_string(),
        ]
    }

    #[golden_test(400, 400)]
    fn tag_filter_empty() -> cosmic::Element<'_, TagFilterMessage> {
        let (filter, _) = TagFilter::new(Value::new(tags()));
        filter.view()
    }

    #[golden_test(400, 400, dark)]
    fn tag_filter_empty_dark() -> cosmic::Element<'_, TagFilterMessage> {
        let (filter, _) = TagFilter::new(Value::new(tags()));
        filter.view()
    }

    #[golden_test(400, 400)]
    fn tag_filter_with_filters() -> cosmic::Element<'_, TagFilterMessage> {
        let (mut filter, _) = TagFilter::new(Value::new(tags()));
        filter.allow_tags.insert("fiction".to_string());
        filter.deny_tags.insert("unread".to_string());
        filter.update_available_tags();
        filter.view()
    }

    #[golden_test(400, 400, dark)]
    fn tag_filter_with_filters_dark() -> cosmic::Element<'_, TagFilterMessage> {
        let (mut filter, _) = TagFilter::new(Value::new(tags()));
        filter.allow_tags.insert("fiction".to_string());
        filter.deny_tags.insert("unread".to_string());
        filter.update_available_tags();
        filter.view()
    }
}
