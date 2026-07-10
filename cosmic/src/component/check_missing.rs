// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Arc;

use cosmic::cosmic_theme;
use cosmic::prelude::*;
use cosmic::task;
use cosmic::theme;
use cosmic::widget;
use cosmic::widget::icon;

use crate::ApplicationModule;
use crate::fl;

#[derive(Debug, Clone)]
pub enum CheckMissingOutput {
    /// Purge completed — parent should refresh the document provider.
    Purged,
    /// User dismissed the dialog.
    Dismissed,
}

#[derive(Debug, Clone)]
pub enum CheckMissingMessage {
    /// Async result from check_missing(false) delivered by the init task.
    Result(Vec<String>),
    Purge,
    Dismiss,
    Out(CheckMissingOutput),
}

/// @feature: admin.check_missing
pub struct CheckMissingComponent {
    files: Option<Vec<String>>,
    application_module: Arc<ApplicationModule>,
}

impl CheckMissingComponent {
    pub fn new(
        application_module: Arc<ApplicationModule>,
    ) -> (Self, Task<cosmic::Action<CheckMissingMessage>>) {
        let module = application_module.clone();
        let init_task = task::future(async move {
            let files = module.check_missing(false).await;
            CheckMissingMessage::Result(files)
        });
        (
            Self {
                files: None,
                application_module,
            },
            init_task.map(cosmic::action::app),
        )
    }

    pub fn update(
        &mut self,
        message: CheckMissingMessage,
    ) -> Task<cosmic::Action<CheckMissingMessage>> {
        match message {
            CheckMissingMessage::Result(files) => {
                self.files = Some(files);
                Task::none()
            }
            CheckMissingMessage::Purge => {
                let application_module = self.application_module.clone();
                task::future(async move {
                    application_module.check_missing(true).await;
                    CheckMissingMessage::Out(CheckMissingOutput::Purged)
                })
                .map(cosmic::action::app)
            }
            CheckMissingMessage::Dismiss => {
                task::message(CheckMissingMessage::Out(CheckMissingOutput::Dismissed))
            }
            CheckMissingMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent")
            }
        }
    }

    pub fn dialog(&self) -> Option<Element<'_, CheckMissingMessage>> {
        let files = self.files.as_ref()?;

        let cosmic_theme::Spacing {
            space_s, space_xs, ..
        } = theme::active().cosmic().spacing;

        let dialog = if files.is_empty() {
            widget::dialog()
                .title(fl!("check-missing-dialog-title"))
                .body(fl!("check-missing-no-missing"))
                .icon(icon::from_name("dialog-information-symbolic").size(64))
                .primary_action(
                    widget::button::suggested(fl!("check-missing-cancel"))
                        .on_press(CheckMissingMessage::Dismiss),
                )
        } else {
            let file_list = widget::column::with_children(
                files
                    .iter()
                    .map(|p| widget::text::monotext(p.as_str()).into())
                    .collect::<Vec<_>>(),
            )
            .spacing(space_xs);

            widget::dialog()
                .title(fl!("check-missing-dialog-title"))
                .body(fl!("check-missing-dialog-body"))
                .icon(icon::from_name("dialog-warning-symbolic").size(64))
                .control(
                    file_list
                        .apply(widget::scrollable::vertical)
                        .height(cosmic::iced::Length::Fixed(300.0))
                        .apply(widget::container)
                        .class(cosmic::theme::Container::Card)
                        .padding(space_s)
                        .width(cosmic::iced::Length::Fill),
                )
                .primary_action(
                    widget::button::destructive(fl!("check-missing-purge"))
                        .on_press(CheckMissingMessage::Purge),
                )
                .secondary_action(
                    widget::button::standard(fl!("check-missing-cancel"))
                        .on_press(CheckMissingMessage::Dismiss),
                )
        };

        Some(dialog.into())
    }
}
