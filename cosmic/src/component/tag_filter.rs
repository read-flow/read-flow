// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashSet;
use std::fmt::Display;

use cosmic::Element;
use cosmic::Task;
use cosmic::widget;
use cosmic::widget::settings;
use provider::r#async::Provider;

use super::provided_state::ProvidedState;
use super::provided_state::ProvidedStateMessage;
use crate::component::tag_pill_filter;
use crate::cosmic_ext::ActionExt;
use crate::fl;
use crate::state::tags::Tags;
use crate::state::tags::TagsState;

pub struct TagFilter<P> {
    all_tags: ProvidedState<P, Vec<String>>,
    tags: TagsState,
    pub allow_tags: HashSet<String>,
    pub deny_tags: HashSet<String>,
}

#[derive(Debug, Clone)]
pub enum TagFilterOutput {
    TagFiltersUpdated,
}

#[derive(Debug, Clone)]
pub enum TagFilterMessage {
    Tags(ProvidedStateMessage<Vec<String>>),
    ClearAllTagFilters,
    CycleTag(String),
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
    /// Returns all known tags, or an empty slice while loading.
    pub fn all_tags(&self) -> &[String] {
        match &self.tags {
            TagsState::Loaded(Tags { all_tags, .. }) => all_tags.as_slice(),
            _ => &[],
        }
    }

    pub fn new(tags_provider: P) -> (Self, Task<cosmic::Action<TagFilterMessage>>) {
        let (all_tags, init_task) = ProvidedState::new(tags_provider);
        (
            Self {
                all_tags,
                tags: TagsState::default(),
                allow_tags: HashSet::new(),
                deny_tags: HashSet::new(),
            },
            init_task.map(ActionExt::map_into),
        )
    }

    pub fn view(&self) -> Element<'_, TagFilterMessage> {
        let tag_pill_filters =
            tag_pill_filter::view(self.all_tags(), &self.allow_tags, &self.deny_tags);

        let mut settings = settings::section::section()
            .title(fl!("document-list-filter-by-tags"))
            .add(tag_pill_filters);

        // Clear all tag filters button
        if !self.allow_tags.is_empty() || !self.deny_tags.is_empty() {
            settings = settings.add(widget::settings::item_row(vec![
                widget::space::horizontal().into(),
                widget::button::text(fl!("document-list-clear-all-tag-filters"))
                    .on_press(TagFilterMessage::ClearAllTagFilters)
                    .into(),
            ]));
        }

        settings.into()
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
            TagFilterMessage::ClearAllTagFilters => {
                self.allow_tags.clear();
                self.deny_tags.clear();
                self.update_available_tags();
                cosmic::task::message(TagFilterMessage::Out(TagFilterOutput::TagFiltersUpdated))
            }
            TagFilterMessage::CycleTag(tag) => {
                if self.allow_tags.contains(&tag) {
                    self.allow_tags.remove(&tag);
                    self.deny_tags.insert(tag);
                } else if self.deny_tags.contains(&tag) {
                    self.deny_tags.remove(&tag);
                } else {
                    self.allow_tags.insert(tag);
                }
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
