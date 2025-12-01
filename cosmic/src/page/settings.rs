// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;
use std::sync::Arc;

use archive_organizer::scan::DirectorySettings;
use archive_organizer::settings::Settings;
use cosmic::Action;
use cosmic::Element;
use cosmic::Task;
use cosmic::cosmic_theme;
use cosmic::task;
use cosmic::theme;
use cosmic::widget;
use cosmic::widget::icon;
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

/// Directory action for editing
///
/// Represents the action to take for a directory in the scan settings.
/// Used in the UI to allow users to select between scanning or ignoring directories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryAction {
    /// Scan the directory and its contents
    Scan,
    /// Ignore the directory during scanning
    Ignore,
}

impl DirectoryAction {
    /// Convert DirectoryAction to DirectorySettings with inheritance
    ///
    /// # Arguments
    /// * `inherit` - Whether settings should be inherited by subdirectories
    ///
    /// # Returns
    /// A DirectorySettings enum variant with the specified inheritance
    pub fn to_settings(self, inherit: bool) -> DirectorySettings {
        match self {
            DirectoryAction::Scan => DirectorySettings::Scan {
                tags: vec![],
                inherit,
            },
            DirectoryAction::Ignore => DirectorySettings::Ignore { inherit },
        }
    }

    /// Convert DirectorySettings to DirectoryAction
    ///
    /// # Arguments
    /// * `settings` - The DirectorySettings to convert from
    ///
    /// # Returns
    /// A DirectoryAction enum variant matching the settings type
    pub fn from_settings(settings: &DirectorySettings) -> Self {
        match settings {
            DirectorySettings::Scan { .. } => DirectoryAction::Scan,
            DirectorySettings::Ignore { .. } => DirectoryAction::Ignore,
        }
    }
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
    /// Directory editing state - Some(path) if editing, None if not editing
    /// Special value "__new_directory__" indicates adding a new directory
    editing_directory: Option<PathBuf>,
    /// Path input for new/editing directory
    new_directory_path: String,
    /// Action selection for new/editing directory (Scan/Ignore)
    new_directory_action: DirectoryAction,
    /// Inheritance setting for new/editing directory
    new_directory_inherit: bool,
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
    /// Directory management messages
    /// Open the directory editor for adding a new directory
    AddDirectory,
    /// Remove a directory from the scan settings
    ///
    /// # Arguments
    /// * `PathBuf` - The path to the directory to remove
    RemoveDirectory(PathBuf),
    /// Open the directory editor for editing an existing directory
    ///
    /// # Arguments
    /// * `PathBuf` - The path to the directory to edit
    EditDirectory(PathBuf),
    /// Update the directory path input in the editor
    ///
    /// # Arguments
    /// * `String` - The new directory path
    UpdateDirectoryPath(String),
    /// Update the directory action selection in the editor
    ///
    /// # Arguments
    /// * `DirectoryAction` - The new action (Scan/Ignore)
    UpdateDirectoryAction(DirectoryAction),
    /// Update the directory inheritance setting in the editor
    ///
    /// # Arguments
    /// * `bool` - Whether to inherit settings to subdirectories
    UpdateDirectoryInherit(bool),
    /// Save the directory being edited/added to settings
    SaveDirectory,
    /// Cancel directory editing and close the editor
    CancelEditDirectory,
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
                editing_directory: None,
                new_directory_path: String::new(),
                new_directory_action: DirectoryAction::Scan,
                new_directory_inherit: true,
            },
            tag_editor_task.map(|action| action.map(SettingsMessage::TagEditor)),
        )
    }

    /// Check if settings have been modified
    ///
    /// Compares the current settings with the original settings to determine
    /// if any changes have been made that need to be saved.
    ///
    /// # Returns
    /// `true` if settings have been modified, `false` otherwise
    fn is_modified(&self) -> bool {
        self.settings.scan.dry_run != self.original_settings.scan.dry_run
            || self.settings.ui.private_mode() != self.original_settings.ui.private_mode()
            || self.settings.ui.private_tags() != self.original_settings.ui.private_tags()
            || self.settings.scan.directories != self.original_settings.scan.directories
    }

    /// Create directory editor component
    ///
    /// Creates the UI component for editing or adding a directory.
    /// Includes path input, action selection (Scan/Ignore), inheritance toggle,
    /// and save/cancel buttons.
    ///
    /// # Returns
    /// An Element containing the directory editor UI
    fn directory_editor_view(&self) -> Element<'_, SettingsMessage> {
        let cosmic_theme::Spacing { space_s, .. } = theme::active().cosmic().spacing;

        let path_input =
            widget::text_input(fl!("settings-directory-path"), &self.new_directory_path)
                .on_input(SettingsMessage::UpdateDirectoryPath);

        // Use radio buttons instead of dropdown to avoid lifetime issues
        let scan_radio = widget::radio(
            widget::text::body(fl!("settings-directory-action-scan-label")),
            DirectoryAction::Scan,
            Some(self.new_directory_action),
            SettingsMessage::UpdateDirectoryAction,
        );

        let ignore_radio = widget::radio(
            widget::text::body(fl!("settings-directory-action-ignore-label")),
            DirectoryAction::Ignore,
            Some(self.new_directory_action),
            SettingsMessage::UpdateDirectoryAction,
        );

        let action_selection = widget::column()
            .push(scan_radio)
            .push(ignore_radio)
            .spacing(space_s);

        let inherit_toggle = widget::toggler(self.new_directory_inherit)
            .on_toggle(SettingsMessage::UpdateDirectoryInherit);

        let save_button = widget::button::suggested(fl!("settings-save-directory"))
            .on_press(SettingsMessage::SaveDirectory);

        let cancel_button = widget::button::standard(fl!("settings-cancel-edit"))
            .on_press(SettingsMessage::CancelEditDirectory);

        let editor_content = widget::column()
            .push(settings::item(fl!("settings-directory-path"), path_input))
            .push(settings::item(
                fl!("settings-directory-action"),
                action_selection,
            ))
            .push(settings::item(
                fl!("settings-directory-inherit"),
                inherit_toggle,
            ))
            .push(
                widget::row()
                    .push(save_button)
                    .push(cancel_button)
                    .spacing(space_s),
            )
            .spacing(space_s);

        widget::container(editor_content).padding(space_s).into()
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

        // Scan directories section with add/edit functionality
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

                let edit_button = widget::button::icon(icon::from_name("edit-symbolic").size(16))
                    .on_press(SettingsMessage::EditDirectory(path.clone().into()))
                    .tooltip(fl!("settings-edit-directory"));

                let remove_button =
                    widget::button::icon(icon::from_name("list-remove-symbolic").size(16))
                        .class(widget::button::ButtonClass::Destructive)
                        .on_press(SettingsMessage::RemoveDirectory(path.clone().into()))
                        .tooltip(fl!("settings-remove-directory"));

                let controls = widget::row()
                    .push(edit_button)
                    .push(remove_button)
                    .spacing(space_s);

                section.add(settings::item(
                    format!("{}", path.display()),
                    widget::row()
                        .push(widget::text::body(action))
                        .push(widget::horizontal_space())
                        .push(controls)
                        .spacing(space_m)
                        .align_y(cosmic::iced::Alignment::Center),
                ))
            },
        );

        let add_button = widget::button::icon(icon::from_name("list-add-symbolic").size(16))
            .class(widget::button::ButtonClass::Suggested)
            .on_press(SettingsMessage::AddDirectory)
            .tooltip(fl!("settings-add-directory"));

        let directories_with_add = widget::column()
            .push(directories_section)
            .push(add_button)
            .spacing(space_s);

        content.push(directories_with_add.into());

        // Show directory editor if editing
        if self.editing_directory.is_some() {
            let is_adding = self
                .editing_directory
                .as_ref()
                .map(|path| path.to_string_lossy() == "__new_directory__")
                .unwrap_or(false);

            let title = if is_adding {
                fl!("settings-add-directory")
            } else {
                fl!("settings-edit-directory")
            };

            let editor_section = settings::section()
                .title(title)
                .add(self.directory_editor_view());
            content.push(editor_section.into());
        }

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
            SettingsMessage::AddDirectory => {
                // Use a special marker path to indicate we're adding a new directory
                // This triggers the directory editor to show with default values
                self.editing_directory = Some(PathBuf::from("__new_directory__"));
                self.new_directory_path = String::new();
                self.new_directory_action = DirectoryAction::Scan;
                self.new_directory_inherit = true;
                Task::none()
            }
            SettingsMessage::EditDirectory(path) => {
                // Load existing directory settings into the editor
                self.editing_directory = Some(path.clone());
                if let Ok(expanded_path) = archive_organizer::ExpandedPath::try_from(path.clone()) {
                    if let Some(dir_settings) = self.settings.scan.directories.get(&expanded_path) {
                        self.new_directory_path = path.to_string_lossy().to_string();
                        self.new_directory_action = DirectoryAction::from_settings(dir_settings);
                        self.new_directory_inherit = dir_settings.inherit();
                    } else {
                        // Fallback to defaults if directory not found
                        self.new_directory_path = path.to_string_lossy().to_string();
                        self.new_directory_action = DirectoryAction::Scan;
                        self.new_directory_inherit = true;
                    }
                } else {
                    // Fallback to defaults if path conversion fails
                    self.new_directory_path = path.to_string_lossy().to_string();
                    self.new_directory_action = DirectoryAction::Scan;
                    self.new_directory_inherit = true;
                }
                Task::none()
            }
            SettingsMessage::UpdateDirectoryPath(path) => {
                // Update the path input field in the directory editor
                self.new_directory_path = path;
                Task::none()
            }
            SettingsMessage::UpdateDirectoryAction(action) => {
                // Update the action selection (Scan/Ignore) in the directory editor
                self.new_directory_action = action;
                Task::none()
            }
            SettingsMessage::UpdateDirectoryInherit(inherit) => {
                // Update the inheritance toggle in the directory editor
                self.new_directory_inherit = inherit;
                Task::none()
            }
            SettingsMessage::SaveDirectory => {
                // Validate and save the directory being edited/added
                if self.new_directory_path.is_empty() {
                    Task::none() // TODO: Show error message for empty path
                } else {
                    let path_buf = PathBuf::from(&self.new_directory_path);
                    let expanded_path =
                        match archive_organizer::ExpandedPath::try_from(path_buf.clone()) {
                            Ok(path) => path,
                            Err(_) => return Task::none(), // TODO: Show error message for invalid path
                        };

                    let dir_settings = self
                        .new_directory_action
                        .to_settings(self.new_directory_inherit);

                    // Check if we're editing an existing directory or adding a new one
                    if let Some(original_path) = &self.editing_directory {
                        if original_path.to_string_lossy() == "__new_directory__" {
                            // Adding a new directory - just insert it
                            self.settings
                                .scan
                                .directories
                                .insert(expanded_path, dir_settings);
                        } else {
                            // Editing an existing directory - remove old entry and add new one
                            if let Ok(expanded_original) =
                                archive_organizer::ExpandedPath::try_from(original_path.clone())
                            {
                                self.settings
                                    .scan
                                    .directories
                                    .shift_remove(&expanded_original);
                            }
                            self.settings
                                .scan
                                .directories
                                .insert(expanded_path, dir_settings);
                        }
                    }

                    self.editing_directory = None;
                    self.save_state = SaveState::Idle;
                    Task::none()
                }
            }
            SettingsMessage::CancelEditDirectory => {
                // Cancel directory editing and reset the editor state
                self.editing_directory = None;
                self.new_directory_path = String::new();
                self.new_directory_action = DirectoryAction::Scan;
                self.new_directory_inherit = true;
                Task::none()
            }
            SettingsMessage::RemoveDirectory(path) => {
                // Remove a directory from the scan settings
                if let Ok(expanded_path) = archive_organizer::ExpandedPath::try_from(path) {
                    self.settings.scan.directories.shift_remove(&expanded_path);
                    self.save_state = SaveState::Idle;
                }
                Task::none()
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
