// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;

use archive_organizer::settings::Settings;
use cosmic::Action;
use cosmic::Element;
use cosmic::Task;
use cosmic::cosmic_theme;
use cosmic::theme;
use cosmic::widget;
use cosmic::widget::settings;

use crate::app::ContextView;
use crate::fl;

pub struct SettingsPage {
    settings: Arc<Settings>,
}

#[derive(Debug, Clone)]
pub enum SettingsOutput {}

#[derive(Debug, Clone)]
pub enum SettingsMessage {
    // Future messages for editing settings will go here
    Out(SettingsOutput),
}

impl SettingsPage {
    pub fn new(settings: Arc<Settings>) -> (Self, Task<Action<SettingsMessage>>) {
        (Self { settings }, Task::none())
    }

    pub fn view(&self) -> Element<'_, SettingsMessage> {
        let cosmic_theme::Spacing { space_s, .. } = theme::active().cosmic().spacing;

        let mut content = Vec::new();

        // Database section
        let database_section = settings::section()
            .title(fl!("settings-database-section"))
            .add(
                widget::row()
                    .push(widget::text::body(fl!("settings-database-location")))
                    .push(widget::horizontal_space())
                    .push(
                        widget::text::body(self.settings.database.url())
                            .font(cosmic::font::Font::MONOSPACE),
                    )
                    .spacing(space_s),
            );
        content.push(database_section.into());

        // Scan section
        let scan_section = settings::section().title(fl!("settings-scan-section")).add(
            widget::row()
                .push(widget::text::body(fl!("settings-scan-dry-run")))
                .push(widget::horizontal_space())
                .push(widget::text::body(if self.settings.scan.dry_run {
                    fl!("settings-enabled")
                } else {
                    fl!("settings-disabled")
                }))
                .spacing(space_s),
        );
        content.push(scan_section.into());

        // Scan directories section
        let directories_section = self.settings.scan.directories.iter().fold(
            settings::section().title(fl!("settings-scan-directories-section")),
            |section, (path, dir_settings)| {
                let action = match dir_settings {
                    archive_organizer::scan::DirectorySettings::Ignore { .. } => {
                        fl!("settings-directory-action-ignore")
                    }
                    archive_organizer::scan::DirectorySettings::Scan { .. } => {
                        fl!("settings-directory-action-scan")
                    }
                };
                section.add(
                    widget::row()
                        .push(
                            widget::text::body(format!("{}", path.display()))
                                .font(cosmic::font::Font::MONOSPACE),
                        )
                        .push(widget::horizontal_space())
                        .push(widget::text::body(action))
                        .spacing(space_s),
                )
            },
        );
        content.push(directories_section.into());

        // UI section
        let ui_section = settings::section().title(fl!("settings-ui-section")).add(
            widget::row()
                .push(widget::text::body(fl!("settings-ui-private-tags")))
                .push(widget::horizontal_space())
                .push(
                    widget::text::body(self.settings.ui.hidden_tags().join(", "))
                        .font(cosmic::font::Font::MONOSPACE),
                )
                .spacing(space_s),
        );
        content.push(ui_section.into());

        settings::view_column(content).into()
    }

    pub fn view_context(&self) -> ContextView<'_, SettingsMessage> {
        ContextView {
            title: fl!("settings-context-title"),
            content: widget::text(fl!("settings-context-placeholder")).into(),
        }
    }

    pub fn update(&mut self, message: SettingsMessage) -> Task<Action<SettingsMessage>> {
        tracing::debug!("received: {message:?}");
        match message {
            SettingsMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent component")
            }
        }
    }
}
