// SPDX-License-Identifier: AGPL-3.0-or-later

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

/// @feature: admin.scan_directories
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
    /// Validation error from the last save attempt, shown until the path changes
    path_error: Option<String>,
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
            path_error: None,
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

    /// Save is only possible once a directory has been selected.
    fn can_save(&self) -> bool {
        self.new_directory_path.is_some()
    }

    pub fn save_button(&self) -> Element<'_, DirectorySettingsFormMessage> {
        widget::button::suggested(fl!("settings-save-directory"))
            .apply_if(self.can_save(), |b| {
                b.on_press(DirectorySettingsFormMessage::SaveDirectory)
            })
            .into()
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
                widget::button::text(fl!("settings-directory-select"))
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
            .add_maybe(self.path_error.as_ref().map(|error| {
                settings::item::builder(error.as_str())
                    .icon(widget::icon::from_name("dialog-error-symbolic").size(ICON_SIZE))
                    .control(widget::Space::new())
            }))
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
    }

    /// Title for the dialog hosting this form ("Add directory" / "Edit directory").
    pub fn title(&self) -> String {
        if self.original_settings.is_none() {
            fl!("settings-add-directory")
        } else {
            fl!("settings-edit-directory")
        }
    }

    pub fn view<'a>(&'a self) -> Element<'a, DirectorySettingsFormMessage> {
        let mut content = Vec::new();

        let editor_section = settings::section();

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
                    self.path_error = None;
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
                let expanded_path = match validate_directory_path(self.new_directory_path.as_ref())
                {
                    Ok(path) => path,
                    Err(error) => {
                        self.path_error = Some(error);
                        return Task::none();
                    }
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

/// Turns the selected directory into an [`ExpandedPath`], or a user-facing
/// (translated) error message when nothing is selected or it can't be expanded.
fn validate_directory_path(path: Option<&FileHandle>) -> Result<ExpandedPath, String> {
    let path = path.ok_or_else(|| fl!("settings-directory-path-required"))?;
    ExpandedPath::try_from(PathBuf::from(path))
        .map_err(|error| fl!("settings-directory-path-invalid", error = error.to_string()))
}

#[cfg(test)]
mod tests {
    use assert4rs::Assert;

    use super::*;

    #[test]
    fn validate_rejects_missing_path() {
        Assert::that(validate_directory_path(None).unwrap_err())
            .is(fl!("settings-directory-path-required"));
    }

    #[test]
    fn validate_accepts_selected_absolute_path() {
        let handle = FileHandle::from(PathBuf::from("/some/books"));
        Assert::that(
            validate_directory_path(Some(&handle))
                .unwrap()
                .to_path_buf(),
        )
        .is(PathBuf::from("/some/books"));
    }
}
