// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;
use std::sync::Arc;

use archive_organizer::Builder;
use archive_organizer::ExpandedPath;
use archive_organizer::scan::DirectorySettings;
use archive_organizer::settings::Settings;
use cosmic::Action;
use cosmic::Apply;
use cosmic::Element;
use cosmic::Task;
use cosmic::cosmic_theme;
use cosmic::iced::Length;
use cosmic::iced_widget::Row;
use cosmic::task;
use cosmic::theme;
use cosmic::widget;
use cosmic::widget::container;
use cosmic::widget::icon;
use cosmic::widget::settings;
use rfd::AsyncFileDialog;
use rfd::FileHandle;

use crate::app::ContextView;
use crate::component::tag_editor::TagEditor;
use crate::component::tag_editor::TagEditorMessage;
use crate::component::tag_editor::TagEditorOutput;
use crate::cosmic_ext::ActionExt;
use crate::document_provider::DocumentProvider;
use crate::fl;
use crate::forms::settings::directory_settings::DirectorySettingsForm;
use crate::forms::settings::directory_settings::DirectorySettingsFormMessage;
use crate::forms::settings::directory_settings::DirectorySettingsFormOutput;

/// State for tracking save status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveState {
    Idle,
    Saving,
    Saved,
    Error(String),
}

pub struct SettingsPage {
    /// Aggregator
    document_provider: Arc<DocumentProvider>,
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
    /// Directory Settings Form
    directory_settings_form: Option<DirectorySettingsForm>,
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
    /// Directory settings form message
    DirectorySettingsForm(DirectorySettingsFormMessage),
    SelectDatabaseLocation,
    SelectedDatabaseLocation(Option<FileHandle>),
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
    /// Save the directory being edited/added to settings
    SaveDirectory(ExpandedPath, DirectorySettings),
    /// Cancel directory editing and close the editor
    CancelEditDirectory,
    /// Output message (for parent component)
    Out(SettingsOutput),
}

impl From<TagEditorMessage> for SettingsMessage {
    fn from(value: TagEditorMessage) -> Self {
        Self::TagEditor(value)
    }
}

impl From<DirectorySettingsFormMessage> for SettingsMessage {
    fn from(value: DirectorySettingsFormMessage) -> Self {
        Self::DirectorySettingsForm(value)
    }
}

impl SettingsPage {
    pub fn new(
        settings: Arc<Settings>,
        document_provider: Arc<DocumentProvider>,
    ) -> (Self, Task<Action<SettingsMessage>>) {
        let document_provider_clone = document_provider.clone();
        let (tag_editor, tag_editor_task) = TagEditor::new(
            Box::new(move || {
                let document_provider = document_provider_clone.clone();
                Box::pin(async move {
                    document_provider
                        .get_all_tags()
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
                document_provider: document_provider.clone(),
                original_settings: settings.clone(),
                settings: (*settings).clone(),
                tag_editor,
                save_state: SaveState::Idle,
                editing_directory: None,
                directory_settings_form: None,
            },
            tag_editor_task.map(ActionExt::map_into),
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
        self.settings != *self.original_settings
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
                Row::with_children(vec![
                    widget::text::body(self.settings.database.url())
                        .font(cosmic::font::Font::MONOSPACE)
                        .into(),
                    widget::button::text("Select")
                        .on_press(SettingsMessage::SelectDatabaseLocation)
                        .into(),
                ]),
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
                    .spacing(space_s)
                    .apply(container)
                    .align_right(Length::Shrink);

                section.add(
                    settings::item_row(vec![
                        widget::text::body(path.display().to_string())
                            .width(Length::FillPortion(3))
                            .into(),
                        widget::text::body(action)
                            .width(Length::FillPortion(1))
                            .into(),
                        controls.width(Length::FillPortion(1)).into(),
                    ])
                    .spacing(space_m)
                    .align_y(cosmic::iced::Alignment::Center),
                )
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
        if let Some(directory_settings_form) = self.directory_settings_form.as_ref() {
            content.push(directory_settings_form.view().map(Into::into));
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
            SettingsMessage::TagEditor(message) => match message {
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
                message => self.tag_editor.update(message).map(ActionExt::map_into),
            },
            SettingsMessage::DirectorySettingsForm(message) => match message {
                DirectorySettingsFormMessage::Out(directory_settings_form_output) => {
                    match directory_settings_form_output {
                        DirectorySettingsFormOutput::Cancelled => {
                            task::message(SettingsMessage::CancelEditDirectory)
                        }
                        DirectorySettingsFormOutput::Ok(expanded_path, directory_settings) => {
                            task::message(SettingsMessage::SaveDirectory(
                                expanded_path,
                                directory_settings,
                            ))
                        }
                    }
                }
                message => {
                    if let Some(directory_form) = self.directory_settings_form.as_mut() {
                        directory_form.update(message).map(ActionExt::map_into)
                    } else {
                        Task::none()
                    }
                }
            },
            SettingsMessage::SelectDatabaseLocation => {
                let path = self
                    .settings
                    .database
                    .url()
                    .parse::<ExpandedPath>()
                    .unwrap()
                    .get_full_path();
                let parent_str = path.parent().map(|path| path.display().to_string());
                let file_name = path
                    .file_name()
                    .map(|file_name| file_name.display().to_string());
                task::future(async move {
                    let directory = AsyncFileDialog::new()
                        .apply_maybe(parent_str, |dialog, path| dialog.set_directory(path))
                        .apply_maybe(file_name, |dialog, file_name| {
                            dialog.set_file_name(file_name)
                        })
                        .pick_file()
                        .await;

                    SettingsMessage::SelectedDatabaseLocation(directory)
                })
            }
            SettingsMessage::SelectedDatabaseLocation(file_handle) => {
                // Only overwrite when some file_handle is returned
                if let Some(file) = file_handle {
                    self.settings
                        .database
                        .set_url(file.path().display().to_string());
                    self.save_state = SaveState::Idle;
                }
                Task::none()
            }
            SettingsMessage::AddDirectory => {
                // Use a special marker path to indicate we're adding a new directory
                // This triggers the directory editor to show with default values
                self.editing_directory = Some(PathBuf::from("__new_directory__"));
                let (directory_settings_form, initialize) =
                    DirectorySettingsForm::new(None, self.document_provider.clone());
                self.directory_settings_form = Some(directory_settings_form);
                initialize.map(ActionExt::map_into)
            }
            SettingsMessage::EditDirectory(path) => {
                // Load existing directory settings into the editor
                self.editing_directory = Some(path.clone());
                let (expanded_path, dir_settings) = if let Ok(expanded_path) =
                    archive_organizer::ExpandedPath::try_from(path.clone())
                {
                    if let Some(dir_settings) = self.settings.scan.directories.get(&expanded_path) {
                        (expanded_path, dir_settings.clone())
                    } else {
                        (expanded_path, DirectorySettings::Ignore { inherit: false })
                    }
                } else {
                    (
                        Default::default(),
                        DirectorySettings::Ignore { inherit: false },
                    )
                };

                let (directory_settings_form, initialize) = DirectorySettingsForm::new(
                    Some((expanded_path, dir_settings)),
                    self.document_provider.clone(),
                );
                self.directory_settings_form = Some(directory_settings_form);
                initialize.map(ActionExt::map_into)
            }
            SettingsMessage::SaveDirectory(expanded_path, dir_settings) => {
                // Validate and save the directory being edited/added
                if expanded_path.as_os_str().is_empty() {
                    Task::none() // TODO: Show error message for empty path
                } else {
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
                                self.settings.scan.directories.remove(&expanded_original);
                            }
                            self.settings
                                .scan
                                .directories
                                .insert(expanded_path, dir_settings);
                        }
                    }

                    self.editing_directory = None;
                    self.directory_settings_form = None;
                    self.save_state = SaveState::Idle;
                    Task::none()
                }
            }
            SettingsMessage::CancelEditDirectory => {
                // Cancel directory editing and reset the editor state
                self.editing_directory = None;
                self.directory_settings_form = None;
                Task::none()
            }
            SettingsMessage::RemoveDirectory(path) => {
                // Remove a directory from the scan settings
                if let Ok(expanded_path) = archive_organizer::ExpandedPath::try_from(path) {
                    self.settings.scan.directories.remove(&expanded_path);
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
