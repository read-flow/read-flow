// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashSet;

use cosmic::Apply;
use cosmic::Element;
use cosmic::cosmic_theme;
use cosmic::theme;
use cosmic::widget;

use crate::component::tag_filter::TagFilterMessage;

/// Renders all tags as clickable pills with three states: neutral, allow (+), deny (−).
///
/// Clicking a pill cycles: neutral → allow → deny → neutral.
/// State lives in the caller's [`TagFilter`] component; this is a pure view.
pub fn view<'a>(
    all_tags: &'a [String],
    allow_tags: &'a HashSet<String>,
    deny_tags: &'a HashSet<String>,
) -> Element<'a, TagFilterMessage> {
    if all_tags.is_empty() {
        return widget::row(vec![]).into();
    }

    let cosmic_theme::Spacing {
        space_xxs,
        space_xs,
        space_s,
        ..
    } = theme::active().cosmic().spacing;

    let mut sorted_tags: Vec<&String> = all_tags.iter().collect();
    sorted_tags.sort_unstable();

    let pills: Vec<Element<'_, TagFilterMessage>> = sorted_tags
        .into_iter()
        .map(|tag| {
            if allow_tags.contains(tag) {
                widget::button::suggested(format!("+ {tag}"))
                    .on_press(TagFilterMessage::CycleTag(tag.clone()))
                    .into()
            } else if deny_tags.contains(tag) {
                widget::button::destructive(format!("− {tag}"))
                    .on_press(TagFilterMessage::CycleTag(tag.clone()))
                    .into()
            } else {
                widget::button::text(tag.as_str())
                    .on_press(TagFilterMessage::CycleTag(tag.clone()))
                    .into()
            }
        })
        .collect();

    widget::flex_row(pills)
        .row_spacing(space_xxs)
        .column_spacing(space_xs)
        .apply(widget::container)
        .padding([space_xxs, space_s])
        .into()
}
