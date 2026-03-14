// SPDX-License-Identifier: GPL-3.0-or-later

use std::mem::take;
use std::path::PathBuf;
use std::sync::Arc;

use cosmic::Action;
use cosmic::Element;
use cosmic::Task;
use cosmic::iced::Length;
use cosmic::task;
use cosmic::widget;
use cosmic::widget::settings;
use cosmic::widget::settings::Section;
use read_flow_core::Builder;
use read_flow_core::ExpandedPath;
use read_flow_core::scan::DirectorySettings;
use rfd::AsyncFileDialog;
use rfd::FileHandle;

use crate::ICON_SIZE;
use crate::component::tag_editor::Orientation;
use crate::component::tag_editor::TagEditor;
use crate::component::tag_editor::TagEditorMessage;
use crate::component::tag_editor::TagEditorOutput;
use crate::cosmic_ext::ActionExt;
use crate::document_provider::DocumentProvider;
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
    document_provider: Arc<DocumentProvider>,
    /// Original settings, or `None` if this is a new entry
    original_settings: Option<(ExpandedPath, DirectorySettings)>,
    /// Tag editor for private tags
    tag_editor: Option<TagEditor<Arc<DocumentProvider>>>,
    /// Path input for new/editing directory
    new_directory_path: Option<FileHandle>,
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
    SelectDirectoryPath,
    SelectedDirectoryPath(Option<FileHandle>),
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
        document_provider: Arc<DocumentProvider>,
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
            document_provider,
            original_settings: settings,
            tag_editor: None,
            new_directory_path: path.get_directory().map(FileHandle::from),
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
        let document_provider = self.document_provider.clone();

        let (tag_editor, tag_editor_task) = TagEditor::new(
            document_provider.clone(),
            self.new_directory_scan_tags.clone(),
            Orientation::Vertical,
            fl!("settings-select-directory-tag"),
            fl!("settings-enter-directory-tag"),
            fl!("settings-no-directory-tags"),
            fl!("settings-remove-directory-tag"),
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
        let path_input = settings::item::builder(fl!("settings-directory-path"))
            .icon(widget::icon::from_name("folder-symbolic").size(ICON_SIZE))
            .control(settings::item_row(vec![
                widget::text_input(
                    fl!("settings-directory-path"),
                    self.new_directory_path
                        .as_ref()
                        .map(|path| path.path().display().to_string())
                        .unwrap_or_default(),
                )
                .into(),
                widget::button::text("Select")
                    .on_press(DirectorySettingsFormMessage::SelectDirectoryPath)
                    .into(),
            ]));

        let action_selection = settings::item::builder(fl!("settings-directory-action"))
            .icon(widget::icon::from_name("system-run-symbolic").size(ICON_SIZE))
            .control(
                settings::item_row(vec![
                    widget::radio(
                        widget::text::body(fl!("settings-directory-action-scan-label")),
                        DirectoryAction::Scan,
                        Some(self.new_directory_action),
                        DirectorySettingsFormMessage::UpdateDirectoryAction,
                    )
                    .into(),
                    widget::radio(
                        widget::text::body(fl!("settings-directory-action-ignore-label")),
                        DirectoryAction::Ignore,
                        Some(self.new_directory_action),
                        DirectorySettingsFormMessage::UpdateDirectoryAction,
                    )
                    .into(),
                ])
                .width(Length::Shrink),
            );

        section
            .add(path_input)
            .add(action_selection)
            .add_maybe(self.tag_editor.as_ref().map(|tag_editor| {
                settings::item::builder(fl!("settings-directory-tags"))
                    .icon(widget::icon::from_name("starred-symbolic").size(ICON_SIZE))
                    .control(tag_editor.view().map(Into::into))
            }))
            .add(
                settings::item::builder(fl!("settings-directory-inherit"))
                    .icon(widget::icon::from_name("application-default-symbolic").size(ICON_SIZE))
                    .toggler(
                        self.new_directory_inherit,
                        DirectorySettingsFormMessage::UpdateDirectoryInherit,
                    ),
            )
            .add(settings::item_row(vec![
                widget::button::suggested(fl!("settings-save-directory"))
                    .on_press(DirectorySettingsFormMessage::SaveDirectory)
                    .into(),
                widget::button::standard(fl!("settings-cancel-edit"))
                    .on_press(DirectorySettingsFormMessage::CancelEditDirectory)
                    .into(),
            ]))
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
            DirectorySettingsFormMessage::SelectDirectoryPath => {
                let path = self.new_directory_path.clone();
                task::future(async move {
                    let directory = AsyncFileDialog::new()
                        .apply_if(path.is_some(), |dialog| {
                            // Unwrap is safe because of `is_some()` above
                            dialog.set_directory(path.as_ref().unwrap().path())
                        })
                        .pick_folder()
                        .await;

                    DirectorySettingsFormMessage::SelectedDirectoryPath(directory)
                })
            }
            DirectorySettingsFormMessage::SelectedDirectoryPath(file_handle) => {
                // Only overwrite when some file_handle is returned
                if let Some(path) = file_handle {
                    self.new_directory_path = Some(path);
                }
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
                if let Some(path) = self.new_directory_path.as_ref() {
                    let path_buf = PathBuf::from(path);
                    let expanded_path = match ExpandedPath::try_from(path_buf) {
                        Ok(path) => path,
                        Err(_) => return Task::none(), // TODO: Show error message for invalid path
                    };

                    let dir_settings = self.take_directory_settings();

                    // reset editor state
                    self.original_settings = None;
                    self.new_directory_path = None;
                    self.new_directory_action = DirectoryAction::Ignore;
                    self.new_directory_inherit = false;

                    task::message(DirectorySettingsFormMessage::Out(
                        DirectorySettingsFormOutput::Ok(expanded_path, dir_settings),
                    ))
                } else {
                    Task::none() // TODO: Show error message for empty path
                }
            }
            DirectorySettingsFormMessage::CancelEditDirectory => {
                // reset the editor state
                self.original_settings = None;
                self.new_directory_path = None;
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
