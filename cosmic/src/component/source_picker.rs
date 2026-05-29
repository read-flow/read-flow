// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::Path;

use cosmic::Apply;
use cosmic::Element;
use cosmic::cosmic_theme;
use cosmic::iced::Length;
use cosmic::iced::alignment::Vertical;
use cosmic::theme;
use cosmic::widget;
use cosmic::widget::button::ButtonClass;

use crate::aggregator::DocumentContent;
use crate::aggregator::DocumentSource;
use crate::client::ClientSelector;
use crate::fl;

fn source_size_label(bytes: i32) -> String {
    const KB: i32 = 1024;
    const MB: i32 = 1024 * KB;
    if bytes == 0 {
        String::new()
    } else if bytes < KB {
        format!("{bytes} B")
    } else if bytes < MB {
        format!("{:.1} KB", bytes as f32 / KB as f32)
    } else {
        format!("{:.1} MB", bytes as f32 / MB as f32)
    }
}

/// A dialog for picking one source from a list.
///
/// Each row shows a file-type icon, location badge, file size, filename, and path.
/// `on_pick` receives the GUID of the chosen source; `on_cancel` is fired by the cancel button.
pub fn source_picker_dialog<'a, Msg>(
    dialog_title: impl Into<String>,
    body: Option<String>,
    sources: Vec<(&'a DocumentContent, &'a DocumentSource)>,
    cover: Option<cosmic::widget::image::Handle>,
    on_pick: impl Fn(String) -> Msg + 'a,
    on_cancel: Msg,
) -> Element<'a, Msg>
where
    Msg: Clone + 'static,
{
    let cosmic_theme::Spacing {
        space_s, space_xs, ..
    } = theme::active().cosmic().spacing;

    let source_buttons: Vec<Element<'_, Msg>> = sources
        .into_iter()
        .map(|(content, source)| {
            let source_path = Path::new(&source.path);
            let filename = source_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&source.path)
                .to_string();

            let location = match &source.client {
                ClientSelector::Local => fl!("document-list-pick-source-local"),
                ClientSelector::Remote(url) => url.host_str().unwrap_or("Remote").to_owned(),
            };

            let left_widget: Element<'_, Msg> = match &cover {
                Some(handle) => cosmic::widget::image(handle.clone())
                    .width(cosmic::iced::Length::Fixed(32.0))
                    .height(cosmic::iced::Length::Fixed(48.0))
                    .content_fit(cosmic::iced::ContentFit::Contain)
                    .into(),
                None => widget::icon::from_name(content.type_.get_file_type_icon())
                    .size(32)
                    .icon()
                    .into(),
            };

            let location_badge = widget::text(location)
                .size(11)
                .apply(widget::container)
                .class(theme::Container::Card)
                .padding([2, 6]);

            let header_row = widget::row::with_children(vec![
                location_badge.into(),
                widget::space::horizontal().into(),
                widget::text(source_size_label(content.size))
                    .size(11)
                    .into(),
            ])
            .spacing(space_xs)
            .align_y(Vertical::Center);

            let text_col = widget::column::with_children(vec![
                header_row.into(),
                widget::text(filename).size(13).into(),
                widget::text(source.path.clone()).size(11).into(),
            ])
            .spacing(space_xs)
            .width(Length::Fill);

            let row = widget::row::with_children(vec![left_widget, text_col.into()])
                .spacing(space_s)
                .align_y(Vertical::Center);

            let guid = source.guid.clone();
            widget::button::custom(row)
                .class(ButtonClass::ListItem)
                .width(Length::Fill)
                .on_press(on_pick(guid))
                .into()
        })
        .collect();

    let controls = widget::column::with_children(source_buttons)
        .spacing(space_s)
        .apply(widget::container)
        .class(theme::Container::Card)
        .padding(space_s)
        .width(Length::Fill);

    let dialog_title: String = dialog_title.into();
    let mut dialog = widget::dialog()
        .title(dialog_title)
        .control(controls)
        .secondary_action(
            widget::button::standard(fl!("document-list-pick-format-cancel")).on_press(on_cancel),
        );

    if let Some(body_text) = body {
        dialog = dialog.body(body_text);
    }

    dialog.into()
}
