// SPDX-License-Identifier: AGPL-3.0-or-later

use cosmic::Apply;
use cosmic::Element;
use cosmic::cosmic_theme;
use cosmic::iced::Length;
use cosmic::theme;
use cosmic::widget;

/// A confirmation dialog for destructive actions.
///
/// Shows a warning icon, title, body, a monospace Card displaying `detail` (e.g. a file path or
/// URL), a destructive confirm button, and a standard cancel button.
pub fn confirm_delete_dialog<'a, Msg: Clone + 'static>(
    title: impl Into<String>,
    body: impl Into<String>,
    detail: &'a str,
    confirm_label: impl Into<String>,
    cancel_label: impl Into<String>,
    on_confirm: Msg,
    on_cancel: Msg,
) -> Element<'a, Msg> {
    let cosmic_theme::Spacing { space_s, .. } = cosmic::theme::active().cosmic().spacing;
    let title: String = title.into();
    let body: String = body.into();
    let confirm_label: String = confirm_label.into();
    let cancel_label: String = cancel_label.into();

    widget::dialog()
        .title(title)
        .body(body)
        .icon(widget::icon::from_name("dialog-warning-symbolic").size(64))
        .control(
            widget::text::monotext(detail)
                .apply(widget::container)
                .class(theme::Container::Card)
                .padding(space_s)
                .width(Length::Fill),
        )
        .primary_action(widget::button::destructive(confirm_label).on_press(on_confirm))
        .secondary_action(widget::button::standard(cancel_label).on_press(on_cancel))
        .into()
}
