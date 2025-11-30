// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;

use archive_organizer::settings::Settings;
use cosmic::Action;
use cosmic::Element;
use cosmic::Task;
use cosmic::cosmic_theme;
use cosmic::task;
use cosmic::theme;
use cosmic::widget;
use cosmic::widget::settings;

use crate::aggregator::Aggregator;
use crate::app::ContextView;
use crate::component::tag_editor::TagEditor;
use crate::component::tag_editor::TagEditorMessage;
use crate::component::tag_editor::TagEditorOutput;
use crate::cosmic_ext::ActionExt;
use crate::fl;

/// State for tracking save status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveState {
    Idle,
    Saving,
    Saved,
    Error(String),
}

pub struct SettingsPage {
    /// Original settings (for comparison)
    original_settings: Arc<Settings>,
    /// Editable copy of settings
    settings: Settings,
    /// Tag editor for private tags
    tag_editor: TagEditor,
    /// Save state
    save_state: SaveState,
}

#[derive(Debug, Clone)]
pub enum SettingsOutput {}

#[derive(Debug, Clone)]
pub enum SettingsMessage {
    /// Toggle dry run mode
    ToggleDryRun(bool),
    /// Toggle private mode
    TogglePrivateMode(bool),
    /// Tag editor message
    TagEditor(TagEditorMessage),
    /// Save settings to file
    Save,
    /// Settings saved successfully
    SaveComplete,
    /// Settings save failed
    SaveError(String),
    /// Output message (for parent component)
    Out(SettingsOutput),
}

impl SettingsPage {
    pub fn new(
        settings: Arc<Settings>,
        aggregator: Arc<Aggregator>,
    ) -> (Self, Task<Action<SettingsMessage>>) {
        let (tag_editor, tag_editor_task) = TagEditor::new(
            Box::new(move || {
                let aggregator = aggregator.clone();
                Box::pin(async move {
                    aggregator
                        .get_file_tags()
                        .await
                        .map_err(|err| format!("{err}"))
                })
            }),
            settings.ui.private_tags().to_vec(),
            fl!("settings-select-private-tag"),
            fl!("settings-enter-private-tag"),
            fl!("settings-no-private-tags"),
            fl!("settings-remove-private-tag"),
        );

        (
            Self {
                original_settings: settings.clone(),
                settings: (*settings).clone(),
                tag_editor,
                save_state: SaveState::Idle,
            },
            tag_editor_task.map(|action| action.map(SettingsMessage::TagEditor)),
        )
    }

    /// Check if settings have been modified
    fn is_modified(&self) -> bool {
        self.settings.scan.dry_run != self.original_settings.scan.dry_run
            || self.settings.ui.private_mode() != self.original_settings.ui.private_mode()
            || self.settings.ui.private_tags() != self.original_settings.ui.private_tags()
    }

    pub fn view(&self) -> Element<'_, SettingsMessage> {
        let cosmic_theme::Spacing {
            space_s, space_m, ..
        } = theme::active().cosmic().spacing;

        let mut content = Vec::new();

        // Database section (read-only)
        let database_section = settings::section()
            .title(fl!("settings-database-section"))
            .add(settings::item(
                fl!("settings-database-location"),
                widget::text::body(self.settings.database.url())
                    .font(cosmic::font::Font::MONOSPACE),
            ));
        content.push(database_section.into());

        // Scan section
        let scan_section =
            settings::section()
                .title(fl!("settings-scan-section"))
                .add(settings::item(
                    fl!("settings-scan-dry-run"),
                    widget::toggler(self.settings.scan.dry_run)
                        .on_toggle(SettingsMessage::ToggleDryRun),
                ));
        content.push(scan_section.into());

        // Scan directories section (read-only for now)
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
                section.add(settings::item(
                    format!("{}", path.display()),
                    widget::text::body(action),
                ))
            },
        );
        content.push(directories_section.into());

        // Save button section
        let save_button = if self.is_modified() {
            widget::button::suggested(fl!("settings-save")).on_press(SettingsMessage::Save)
        } else {
            widget::button::standard(fl!("settings-save"))
        };

        let save_status = match &self.save_state {
            SaveState::Idle => widget::text(""),
            SaveState::Saving => widget::text(fl!("settings-saving")),
            SaveState::Saved => widget::text(fl!("settings-saved")),
            SaveState::Error(err) => {
                widget::text(format!("{}: {}", fl!("settings-save-error"), err))
            }
        };

        let save_section = widget::row()
            .push(save_button)
            .push(widget::horizontal_space())
            .push(save_status)
            .spacing(space_m)
            .padding(space_s);
        content.push(save_section.into());

        settings::view_column(content).into()
    }

    pub fn view_context(&self) -> ContextView<'_, SettingsMessage> {
        let cosmic_theme::Spacing { space_s, .. } = theme::active().cosmic().spacing;

        // UI Privacy section
        let ui_section = settings::section()
            .title(fl!("settings-ui-section"))
            .add(settings::item(
                fl!("settings-ui-private-mode"),
                widget::toggler(self.settings.ui.private_mode())
                    .on_toggle(SettingsMessage::TogglePrivateMode),
            ))
            .add(self.tag_editor.view().map(SettingsMessage::TagEditor));

        let content = widget::column().push(ui_section).spacing(space_s).into();

        ContextView {
            title: fl!("settings-context-title"),
            content,
        }
    }

    pub fn update(&mut self, message: SettingsMessage) -> Task<Action<SettingsMessage>> {
        tracing::debug!("received: {message:?}");
        match message {
            SettingsMessage::ToggleDryRun(value) => {
                self.settings.scan.set_dry_run(value);
                self.save_state = SaveState::Idle;
                Task::none()
            }
            SettingsMessage::TogglePrivateMode(value) => {
                self.settings.ui.set_private_mode(value);
                self.save_state = SaveState::Idle;
                Task::none()
            }
            SettingsMessage::TagEditor(tag_msg) => {
                // Handle output messages from tag editor
                match tag_msg {
                    TagEditorMessage::Out(message) => match message {
                        TagEditorOutput::TagsUpdated(tags) => {
                            self.settings.ui.set_private_tags(tags);
                            self.save_state = SaveState::Idle;
                            Task::none()
                        }
                        TagEditorOutput::TagAdded(_) | TagEditorOutput::TagRemoved(_) => {
                            // These are handled via TagsUpdated
                            Task::none()
                        }
                    },
                    tag_msg => self
                        .tag_editor
                        .update(tag_msg)
                        .map(|action| action.map(SettingsMessage::TagEditor)),
                }
            }
            SettingsMessage::Save => {
                self.save_state = SaveState::Saving;
                let settings = self.settings.clone();
                task::future(async move {
                    match archive_organizer::settings::save(&settings) {
                        Ok(()) => SettingsMessage::SaveComplete,
                        Err(e) => SettingsMessage::SaveError(e.to_string()),
                    }
                })
            }
            SettingsMessage::SaveComplete => {
                self.save_state = SaveState::Saved;
                // Update original settings to reflect saved state
                self.original_settings = Arc::new(self.settings.clone());
                Task::none()
            }
            SettingsMessage::SaveError(error) => {
                self.save_state = SaveState::Error(error);
                Task::none()
            }
            SettingsMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent component")
            }
        }
    }
}
