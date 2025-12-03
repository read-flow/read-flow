// SPDX-License-Identifier: GPL-3.0-or-later

use std::mem::take;
use std::path::PathBuf;
use std::sync::Arc;

use archive_organizer::ExpandedPath;
use archive_organizer::scan::DirectorySettings;
use cosmic::Action;
use cosmic::Element;
use cosmic::Task;
use cosmic::cosmic_theme;
use cosmic::task;
use cosmic::theme;
use cosmic::widget;
use cosmic::widget::settings;
use cosmic::widget::settings::Section;

use crate::aggregator::Aggregator;
use crate::component::tag_editor::TagEditor;
use crate::component::tag_editor::TagEditorMessage;
use crate::component::tag_editor::TagEditorOutput;
use crate::cosmic_ext::ActionExt;
use crate::fl;

/// Directory action for editing
///
/// Represents the action to take for a directory in the scan settings.
/// Used in the UI to allow users to select between scanning or ignoring directories.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DirectoryAction {
    /// Scan the directory and its contents
    Scan,
    /// Ignore the directory during scanning
    #[default]
    Ignore,
}

pub struct DirectorySettingsForm {
    aggregator: Arc<Aggregator>,
    /// Original settings, or `None` if this is a new entry
    original_settings: Option<(ExpandedPath, DirectorySettings)>,
    /// Tag editor for private tags
    tag_editor: Option<TagEditor>,
    /// Path input for new/editing directory
    new_directory_path: String,
    /// Action selection for new/editing directory (Scan/Ignore)
    new_directory_action: DirectoryAction,
    /// Inheritance setting for new/editing directory
    new_directory_inherit: bool,
    /// Tags for the Scan action
    new_directory_scan_tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum DirectorySettingsFormOutput {
    Cancelled,
    Ok(ExpandedPath, DirectorySettings),
}

#[derive(Debug, Clone)]
pub enum DirectorySettingsFormMessage {
    /// Tag editor message
    TagEditor(TagEditorMessage),
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
    Out(DirectorySettingsFormOutput),
}

impl From<TagEditorMessage> for DirectorySettingsFormMessage {
    fn from(source: TagEditorMessage) -> Self {
        DirectorySettingsFormMessage::TagEditor(source)
    }
}

impl DirectorySettingsForm {
    pub fn new(
        settings: Option<(ExpandedPath, DirectorySettings)>,
        aggregator: Arc<Aggregator>,
    ) -> (Self, Task<Action<DirectorySettingsFormMessage>>) {
        let (path, action, inherit, tags) = match settings.clone() {
            Some((path, DirectorySettings::Scan { inherit, tags })) => {
                (path, DirectoryAction::Scan, inherit, Some(tags))
            }
            Some((path, DirectorySettings::Ignore { inherit })) => {
                (path, DirectoryAction::Ignore, inherit, None)
            }
            _ => (Default::default(), DirectoryAction::Ignore, false, None),
        };

        let mut form = Self {
            aggregator,
            original_settings: settings,
            tag_editor: None,
            new_directory_path: format!("{path}"),
            new_directory_action: action,
            new_directory_inherit: inherit,
            new_directory_scan_tags: tags.unwrap_or(vec![]),
        };

        let tag_editor_actions = form.create_or_destroy_tag_editor();

        (form, tag_editor_actions)
    }

    /// Constructs `[DirectorySettings]` from the fields of this form and clears the corresponding fields.
    pub fn take_directory_settings(&mut self) -> DirectorySettings {
        match take(&mut self.new_directory_action) {
            DirectoryAction::Scan => DirectorySettings::Scan {
                tags: take(&mut self.new_directory_scan_tags),
                inherit: take(&mut self.new_directory_inherit),
            },
            DirectoryAction::Ignore => {
                self.new_directory_scan_tags.clear();
                DirectorySettings::Ignore {
                    inherit: take(&mut self.new_directory_inherit),
                }
            }
        }
    }

    fn create_or_destroy_tag_editor(&mut self) -> Task<Action<DirectorySettingsFormMessage>> {
        match self.new_directory_action {
            DirectoryAction::Scan => self.create_tag_editor(),
            DirectoryAction::Ignore => {
                self.tag_editor = None;
                Task::none()
            }
        }
    }

    fn create_tag_editor(&mut self) -> Task<Action<DirectorySettingsFormMessage>> {
        let aggregator = self.aggregator.clone();

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
            self.new_directory_scan_tags.clone(),
            fl!("settings-select-private-tag"),
            fl!("settings-enter-private-tag"),
            fl!("settings-no-private-tags"),
            fl!("settings-remove-private-tag"),
        );

        self.tag_editor = Some(tag_editor);
        tag_editor_task.map(ActionExt::map_into)
    }

    /// Create directory editor component
    ///
    /// Creates the UI component for editing or adding a directory.
    /// Includes path input, action selection (Scan/Ignore), inheritance toggle,
    /// and save/cancel buttons.
    ///
    /// # Returns
    /// An Element containing the directory editor UI
    fn directory_editor_view<'a>(
        &'a self,
        section: Section<'a, DirectorySettingsFormMessage>,
    ) -> Section<'a, DirectorySettingsFormMessage> {
        let cosmic_theme::Spacing { space_s, .. } = theme::active().cosmic().spacing;

        let path_input =
            widget::text_input(fl!("settings-directory-path"), &self.new_directory_path)
                .on_input(DirectorySettingsFormMessage::UpdateDirectoryPath);

        // Use radio buttons instead of dropdown to avoid lifetime issues
        let scan_radio = widget::radio(
            widget::text::body(fl!("settings-directory-action-scan-label")),
            DirectoryAction::Scan,
            Some(self.new_directory_action),
            DirectorySettingsFormMessage::UpdateDirectoryAction,
        );

        let ignore_radio = widget::radio(
            widget::text::body(fl!("settings-directory-action-ignore-label")),
            DirectoryAction::Ignore,
            Some(self.new_directory_action),
            DirectorySettingsFormMessage::UpdateDirectoryAction,
        );

        let action_selection = widget::column()
            .push(scan_radio)
            .push(ignore_radio)
            .spacing(space_s);

        let tag_editor = self
            .tag_editor
            .as_ref()
            .map(|tag_editor| tag_editor.view().map(Into::into));

        let inherit_toggle = widget::toggler(self.new_directory_inherit)
            .on_toggle(DirectorySettingsFormMessage::UpdateDirectoryInherit);

        let save_button = widget::button::suggested(fl!("settings-save-directory"))
            .on_press(DirectorySettingsFormMessage::SaveDirectory);

        let cancel_button = widget::button::standard(fl!("settings-cancel-edit"))
            .on_press(DirectorySettingsFormMessage::CancelEditDirectory);

        section
            .add(settings::item(fl!("settings-directory-path"), path_input))
            .add(settings::item(
                fl!("settings-directory-action"),
                action_selection,
            ))
            .add_maybe(
                tag_editor
                    .map(|tag_editor| settings::item(fl!("settings-directory-tags"), tag_editor)),
            )
            .add(settings::item(
                fl!("settings-directory-inherit"),
                inherit_toggle,
            ))
            .add(
                widget::row()
                    .push(save_button)
                    .push(cancel_button)
                    .spacing(space_s),
            )
    }

    pub fn view<'a>(&'a self) -> Element<'a, DirectorySettingsFormMessage> {
        let mut content = Vec::new();

        let is_adding = self.original_settings.is_none();

        let title = if is_adding {
            fl!("settings-add-directory")
        } else {
            fl!("settings-edit-directory")
        };

        let editor_section = settings::section().title(title);

        content.push(self.directory_editor_view(editor_section).into());

        settings::view_column(content).into()
    }

    pub fn update(
        &mut self,
        message: DirectorySettingsFormMessage,
    ) -> Task<Action<DirectorySettingsFormMessage>> {
        tracing::debug!("received: {message:?}");
        match message {
            DirectorySettingsFormMessage::TagEditor(tag_msg) => {
                // Handle output messages from tag editor
                match tag_msg {
                    TagEditorMessage::Out(message) => match message {
                        TagEditorOutput::TagsUpdated(tags) => {
                            self.new_directory_scan_tags = tags;
                            Task::none()
                        }
                        TagEditorOutput::TagAdded(_) | TagEditorOutput::TagRemoved(_) => {
                            // These are handled via TagsUpdated
                            Task::none()
                        }
                    },
                    tag_msg => self
                        .tag_editor
                        .as_mut()
                        .map(|tag_editor| tag_editor.update(tag_msg).map(ActionExt::map_into))
                        .unwrap_or_else(Task::none),
                }
            }
            DirectorySettingsFormMessage::UpdateDirectoryPath(path) => {
                // Update the path input field in the directory editor
                self.new_directory_path = path;
                Task::none()
            }
            DirectorySettingsFormMessage::UpdateDirectoryAction(action) => {
                // Update the action selection (Scan/Ignore) in the directory editor
                self.new_directory_action = action;
                self.create_or_destroy_tag_editor()
            }
            DirectorySettingsFormMessage::UpdateDirectoryInherit(inherit) => {
                // Update the inheritance toggle in the directory editor
                self.new_directory_inherit = inherit;
                Task::none()
            }
            DirectorySettingsFormMessage::SaveDirectory => {
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

                    let dir_settings = self.take_directory_settings();

                    // reset editor state
                    self.original_settings = None;
                    self.new_directory_path = String::new();
                    self.new_directory_action = DirectoryAction::Ignore;
                    self.new_directory_inherit = false;

                    task::message(DirectorySettingsFormMessage::Out(
                        DirectorySettingsFormOutput::Ok(expanded_path, dir_settings),
                    ))
                }
            }
            DirectorySettingsFormMessage::CancelEditDirectory => {
                // reset the editor state
                self.original_settings = None;
                self.new_directory_path = String::new();
                self.new_directory_action = DirectoryAction::Ignore;
                self.new_directory_inherit = false;

                task::message(DirectorySettingsFormMessage::Out(
                    DirectorySettingsFormOutput::Cancelled,
                ))
            }
            DirectorySettingsFormMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent component")
            }
        }
    }
}
