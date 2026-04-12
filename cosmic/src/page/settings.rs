// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;
use std::sync::Arc;

use cosmic::Action;
use cosmic::Apply;
use cosmic::Element;
use cosmic::Task;
use cosmic::Theme;
use cosmic::cosmic_theme;
use cosmic::iced::Length;
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::alignment::Vertical;
use cosmic::task;
use cosmic::theme;
use cosmic::widget;
use cosmic::widget::container;
use cosmic::widget::icon;
use provider::r#async::HasSetExpired;
use read_flow_core::Builder;
use read_flow_core::ExpandedPath;
use read_flow_core::scan::DirectorySettings;
use read_flow_core::scan::DocumentType;
use read_flow_core::settings::HashedPassword;
use read_flow_core::settings::Settings;
use rfd::AsyncFileDialog;
use rfd::FileHandle;

use crate::ApplicationModule;
use crate::ICON_SIZE;
use crate::app::ContextView;
use crate::component::tag_editor::Orientation;
use crate::component::tag_editor::TagEditor;
use crate::component::tag_editor::TagEditorMessage;
use crate::component::tag_editor::TagEditorOutput;
use crate::cosmic_ext::ActionExt;
use crate::document_provider::DocumentProvider;
use crate::fl;
use crate::forms::settings::authorized_user::AuthorizedUserForm;
use crate::forms::settings::authorized_user::AuthorizedUserFormMessage;
use crate::forms::settings::authorized_user::AuthorizedUserFormOutput;
use crate::forms::settings::directory_settings::DirectorySettingsForm;
use crate::forms::settings::directory_settings::DirectorySettingsFormMessage;
use crate::forms::settings::directory_settings::DirectorySettingsFormOutput;
use crate::layout::layout;
use crate::page::Page;

#[derive(Debug, Clone, PartialEq, Eq)]
enum EditState<T> {
    Idle,
    Adding,
    Editing(T),
}

/// State for tracking save status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveState {
    Idle,
    Saving,
    Saved,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SettingsSection {
    #[default]
    Overview,
    Database,
    Client,
    Scan,
    Server,
}

pub struct SettingsPage {
    /// Application module, to refresh settings on save
    application_module: Arc<ApplicationModule>,
    /// Aggregator
    document_provider: Arc<DocumentProvider>,
    /// Original settings (for comparison)
    original_settings: Arc<Settings>,
    /// Editable copy of settings
    settings: Settings,
    /// Tag editor for private tags
    tag_editor: TagEditor<Arc<DocumentProvider>>,
    /// Save state
    save_state: SaveState,
    /// Directory editing state
    editing_directory: EditState<PathBuf>,
    /// Directory Settings Form
    directory_settings_form: Option<DirectorySettingsForm>,
    /// Authorized User Form
    authorized_user_form: Option<AuthorizedUserForm>,
    /// Currently selected settings section
    selected_section: SettingsSection,
}

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
    /// Client settings
    SelectClientDownloadFolder,
    SelectedClientDownloadFolder(Option<FileHandle>),
    /// Server settings
    SelectServerDownloadFolder,
    SelectedServerDownloadFolder(Option<FileHandle>),
    /// Server settings authorized users
    AuthorizedUserForm(AuthorizedUserFormMessage),
    AddAuthorizedUser,
    EditAuthorizedUser(String),
    DeleteAuthorizedUser(String),
    /// Save settings to file
    Save,
    /// Settings saved successfully
    SaveComplete,
    /// Settings save failed
    SaveError(String),
    /// Toggle a document file type in the scan extensions list
    ToggleDocumentType(DocumentType, bool),
    /// Switch to a different settings section
    SectionChanged(SettingsSection),
    /// No-op message
    Noop,

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

impl From<AuthorizedUserFormMessage> for SettingsMessage {
    fn from(value: AuthorizedUserFormMessage) -> Self {
        Self::AuthorizedUserForm(value)
    }
}

impl SettingsPage {
    pub fn new(
        application_module: Arc<ApplicationModule>,
        document_provider: Arc<DocumentProvider>,
    ) -> (Self, Task<Action<SettingsMessage>>) {
        let settings: Arc<Settings> = Arc::new(
            Settings::extract_from(application_module.config_path()).expect("settings are present"),
        );
        let document_provider_clone = document_provider.clone();
        let (tag_editor, tag_editor_task) = TagEditor::new(
            document_provider_clone.clone(),
            settings.ui.private_tags().to_vec(),
            Orientation::Vertical,
            fl!("settings-select-private-tag"),
            fl!("settings-enter-private-tag"),
            fl!("settings-no-private-tags"),
            fl!("settings-remove-private-tag"),
        );

        (
            Self {
                application_module,
                document_provider: document_provider.clone(),
                original_settings: settings.clone(),
                settings: (*settings).clone(),
                tag_editor,
                save_state: SaveState::Idle,
                editing_directory: EditState::Idle,
                directory_settings_form: None,
                authorized_user_form: None,
                selected_section: SettingsSection::default(),
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

    fn can_be_saved(&self) -> bool {
        self.authorized_user_form.is_none() && self.directory_settings_form.is_none()
    }

    fn view_authorized_user_input<'a>(
        &'a self,
        user_id: &'a String,
        passphrase: &'a HashedPassword,
    ) -> Element<'a, SettingsMessage> {
        let is_editing = self.is_editing_authorized_user(user_id);

        widget::settings::item::builder(user_id)
            .icon(widget::icon::from_name("avatar-default-symbolic").size(ICON_SIZE))
            .control(widget::settings::item_row(vec![
                widget::text_input("", format!("{passphrase}")).into(),
                widget::button::icon(
                    widget::icon::from_name(if is_editing {
                        "edit-clear-symbolic"
                    } else {
                        "edit-symbolic"
                    })
                    .size(ICON_SIZE),
                )
                .apply_if(!is_editing, |button| {
                    button.on_press(SettingsMessage::EditAuthorizedUser(user_id.clone()))
                })
                .into(),
                widget::button::icon(icon::from_name("list-remove-symbolic").size(ICON_SIZE))
                    .on_press(SettingsMessage::DeleteAuthorizedUser(user_id.clone()))
                    .class(widget::button::ButtonClass::Destructive)
                    .into(),
            ]))
            .into()
    }

    fn is_editing_authorized_user<'a>(&'a self, user_id: &'a String) -> bool {
        self.authorized_user_form
            .as_ref()
            .map(|form| form.original_user_id.as_ref())
            .unwrap_or(None)
            == Some(user_id)
    }

    fn view_overview(&self) -> Vec<Element<'_, SettingsMessage>> {
        [
            (
                SettingsSection::Database,
                fl!("settings-database-section"),
                fl!("settings-database-section-description"),
                "package-x-generic-symbolic",
            ),
            (
                SettingsSection::Client,
                fl!("settings-client-section"),
                fl!("settings-client-section-description"),
                "folder-download-symbolic",
            ),
            (
                SettingsSection::Scan,
                fl!("settings-scan-section"),
                fl!("settings-scan-section-description"),
                "system-search-symbolic",
            ),
            (
                SettingsSection::Server,
                fl!("settings-server-section"),
                fl!("settings-server-section-description"),
                "network-server-symbolic",
            ),
        ]
        .into_iter()
        .map(|(s, label, description, icon_name)| {
            widget::settings::section()
                .add(
                    widget::settings::item::builder(label)
                        .description(description)
                        .icon(widget::icon::from_name(icon_name).size(ICON_SIZE))
                        .control(widget::icon::from_name("go-next-symbolic").size(ICON_SIZE))
                        .apply(widget::mouse_area)
                        .on_press(SettingsMessage::SectionChanged(s)),
                )
                .into()
        })
        .collect()
    }

    fn view_save_row(&self) -> Element<'_, SettingsMessage> {
        let cosmic_theme::Spacing {
            space_s, space_m, ..
        } = theme::active().cosmic().spacing;

        let save_button = if self.is_modified() && self.can_be_saved() {
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

        widget::Row::new()
            .push(save_button)
            .push(widget::space::horizontal())
            .push(save_status)
            .spacing(space_m)
            .padding(space_s)
            .into()
    }

    fn view_section_database(&self) -> Vec<Element<'_, SettingsMessage>> {
        let database_section = widget::settings::section()
            .title(fl!("settings-database-section"))
            .add(
                widget::settings::item::builder(fl!("settings-database-location"))
                    .description(fl!("settings-database-location-description"))
                    .icon(widget::icon::from_name("package-x-generic-symbolic").size(ICON_SIZE))
                    .control(
                        widget::settings::item_row(vec![
                            widget::text::monotext(self.settings.database.url().to_string()).into(),
                            widget::button::text("Select")
                                .on_press(SettingsMessage::SelectDatabaseLocation)
                                .into(),
                        ])
                        .align_y(Vertical::Center)
                        .width(Length::Shrink),
                    ),
            );

        vec![
            widget::text::title2(fl!("settings-database-section")).into(),
            database_section.into(),
        ]
    }

    fn view_section_client(&self) -> Vec<Element<'_, SettingsMessage>> {
        let client_section = widget::settings::section()
            .title(fl!("settings-client-section"))
            .add(
                widget::settings::item::builder(fl!("settings-client-download-folder"))
                    .description(fl!("settings-client-download-folder-description"))
                    .icon(widget::icon::from_name("folder-download-symbolic").size(ICON_SIZE))
                    .control(
                        widget::settings::item_row(vec![
                            widget::text::monotext(format!(
                                "{}",
                                self.settings.client.download_folder.display()
                            ))
                            .into(),
                            widget::button::text("Select")
                                .on_press(SettingsMessage::SelectClientDownloadFolder)
                                .into(),
                        ])
                        .align_y(Vertical::Center)
                        .width(Length::Shrink),
                    ),
            );

        vec![
            widget::text::title2(fl!("settings-client-section")).into(),
            client_section.into(),
        ]
    }

    fn view_section_scan(&self) -> Vec<Element<'_, SettingsMessage>> {
        let scan_section = widget::settings::section()
            .title(fl!("settings-scan-section"))
            .add(widget::text::body(fl!("settings-scan-description")).width(Length::Fill))
            .add(
                widget::settings::item::builder(fl!("settings-scan-dry-run"))
                    .description(fl!("settings-scan-dry-run-description"))
                    .icon(widget::icon::from_name("system-run-symbolic").size(ICON_SIZE))
                    .toggler(self.settings.scan.dry_run, SettingsMessage::ToggleDryRun),
            );

        let file_types_section = DocumentType::all().iter().fold(
            widget::settings::section()
                .title(fl!("settings-scan-file-types-section"))
                .add(
                    widget::text::body(fl!("settings-scan-file-types-description"))
                        .width(Length::Fill),
                ),
            |section, doc_type| {
                let enabled = self.settings.scan.extensions.contains(doc_type);
                let doc_type_clone = doc_type.clone();
                section.add(
                    widget::settings::item::builder(format!(".{}", doc_type.as_str()))
                        .description(doc_type.label())
                        .toggler(enabled, move |v| {
                            SettingsMessage::ToggleDocumentType(doc_type_clone.clone(), v)
                        }),
                )
            },
        );

        let directories_section = self
            .settings
            .scan
            .directories
            .iter()
            .fold(
                widget::settings::section().title(fl!("settings-scan-directories-section")),
                |section, (path, dir_settings)| section.add(view_directory(path, dir_settings)),
            )
            .add(widget::settings::item_row(vec![
                widget::space::horizontal()
                    .width(Length::FillPortion(5))
                    .into(),
                widget::button::icon(icon::from_name("list-add-symbolic").size(ICON_SIZE))
                    .class(widget::button::ButtonClass::Suggested)
                    .on_press(SettingsMessage::AddDirectory)
                    .tooltip(fl!("settings-add-directory"))
                    .apply(widget::container)
                    .width(Length::FillPortion(1))
                    .align_x(Horizontal::Right)
                    .into(),
            ]));

        let mut items: Vec<Element<'_, SettingsMessage>> = vec![
            widget::text::title2(fl!("settings-scan-section")).into(),
            scan_section.into(),
            file_types_section.into(),
            directories_section.into(),
        ];

        if let Some(form) = self.directory_settings_form.as_ref() {
            items.push(form.view().map(Into::into));
        }

        items
    }

    fn view_section_server(&self) -> Vec<Element<'_, SettingsMessage>> {
        let server_section = widget::settings::section()
            .title(fl!("settings-server-section"))
            .add(widget::text::body(fl!("settings-server-description")).width(Length::Fill))
            .add(
                widget::settings::item::builder(fl!("settings-server-download-folder"))
                    .description(fl!("settings-server-download-folder-description"))
                    .icon(widget::icon::from_name("folder-download-symbolic").size(ICON_SIZE))
                    .control(
                        widget::settings::item_row(vec![
                            widget::text::monotext(format!(
                                "{}",
                                self.settings.server.download_folder.display()
                            ))
                            .into(),
                            widget::button::text("Select")
                                .on_press(SettingsMessage::SelectServerDownloadFolder)
                                .into(),
                        ])
                        .align_y(Vertical::Center)
                        .width(Length::Shrink),
                    ),
            );

        let authorized_users_section = self
            .settings
            .server
            .authorized_users
            .iter()
            .enumerate()
            .fold(
                widget::settings::section()
                    .title(fl!("settings-server-authorized-users"))
                    .add(
                        widget::text::body(fl!("settings-server-authorized-users-description"))
                            .width(Length::Fill),
                    ),
                |acc, (_index, (user_id, hashed_password))| {
                    acc.add(self.view_authorized_user_input(user_id, hashed_password))
                },
            )
            .add(widget::settings::item_row(vec![
                widget::space::horizontal()
                    .width(Length::FillPortion(5))
                    .into(),
                widget::button::icon(widget::icon::from_name("list-add-symbolic").size(ICON_SIZE))
                    .class(widget::button::ButtonClass::Suggested)
                    .on_press(SettingsMessage::AddAuthorizedUser)
                    .tooltip(fl!("settings-server-add-authorized-user"))
                    .apply(widget::container)
                    .width(Length::FillPortion(1))
                    .align_x(Horizontal::Right)
                    .into(),
            ]));

        let mut items: Vec<Element<'_, SettingsMessage>> = vec![
            widget::text::title2(fl!("settings-server-section")).into(),
            server_section.into(),
            authorized_users_section.into(),
        ];

        if let Some(form) = self.authorized_user_form.as_ref() {
            items.push(form.view().map(Into::into));
        }

        items
    }
}

impl Page for SettingsPage {
    type Message = SettingsMessage;

    fn view(&self) -> Element<'_, SettingsMessage> {
        let mut items: Vec<Element<'_, SettingsMessage>> = match self.selected_section {
            SettingsSection::Overview => {
                let mut col = self.view_overview();
                col.insert(0, widget::text::title2(fl!("settings-page-title")).into());
                col
            }
            ref section => {
                let back_button = widget::Row::new()
                    .push(
                        widget::button::link(fl!("settings-back"))
                            .on_press(SettingsMessage::SectionChanged(SettingsSection::Overview)),
                    )
                    .into();

                let mut items = vec![back_button];
                items.extend(match section {
                    SettingsSection::Database => self.view_section_database(),
                    SettingsSection::Client => self.view_section_client(),
                    SettingsSection::Scan => self.view_section_scan(),
                    SettingsSection::Server => self.view_section_server(),
                    SettingsSection::Overview => unreachable!(),
                });
                items
            }
        };

        if self.selected_section != SettingsSection::Overview {
            items.push(self.view_save_row());
        }

        widget::scrollable::vertical(layout(widget::settings::view_column(items)))
            .width(Length::Fill)
            .height(Length::Fill)
            .apply(widget::container)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Horizontal::Center)
            .align_y(Vertical::Top)
            .into()
    }

    fn view_context(&self) -> ContextView<'_, SettingsMessage> {
        let ui_section = widget::settings::section()
            .title(fl!("settings-ui-section"))
            .add(
                widget::settings::item::builder(fl!("settings-ui-private-mode"))
                    .description(fl!("settings-ui-private-mode-description"))
                    .icon(
                        widget::icon::from_name("preferences-system-privacy-symbolic")
                            .size(ICON_SIZE),
                    )
                    .toggler(
                        self.settings.ui.private_mode(),
                        SettingsMessage::TogglePrivateMode,
                    ),
            )
            .add(
                widget::settings::item::builder(fl!("settings-ui-private-tags"))
                    .description(fl!("settings-ui-private-tags-description"))
                    .icon(widget::icon::from_name("starred-symbolic").size(ICON_SIZE))
                    .flex_control(self.tag_editor.view().map(SettingsMessage::TagEditor)),
            );

        ContextView {
            title: fl!("settings-context-title"),
            content: widget::settings::view_column(vec![ui_section.into(), self.view_save_row()])
                .into(),
        }
    }

    fn update(&mut self, message: SettingsMessage) -> Task<Action<SettingsMessage>> {
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
                let path = self.settings.database.url().get_full_path();
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
                        .set_url(file.path().to_path_buf().try_into().unwrap());
                    self.save_state = SaveState::Idle;
                }
                Task::none()
            }
            SettingsMessage::SelectClientDownloadFolder => {
                let download_folder = self.settings.client.download_folder.clone();

                task::future(async move {
                    let directory = AsyncFileDialog::new()
                        .set_directory(download_folder)
                        .set_can_create_directories(true)
                        .pick_folder()
                        .await;

                    SettingsMessage::SelectedClientDownloadFolder(directory)
                })
            }
            SettingsMessage::SelectedClientDownloadFolder(file_handle) => {
                if let Some(file) = file_handle {
                    self.settings.client.download_folder =
                        file.path().to_path_buf().try_into().unwrap();
                    self.save_state = SaveState::Idle;
                }
                Task::none()
            }
            SettingsMessage::SelectServerDownloadFolder => {
                let download_folder = self.settings.server.download_folder.clone();

                task::future(async move {
                    let directory = AsyncFileDialog::new()
                        .set_directory(download_folder)
                        .set_can_create_directories(true)
                        .pick_folder()
                        .await;

                    SettingsMessage::SelectedServerDownloadFolder(directory)
                })
            }
            SettingsMessage::SelectedServerDownloadFolder(file_handle) => {
                // Only overwrite when some file_handle is returned
                if let Some(file) = file_handle {
                    self.settings.server.download_folder =
                        file.path().to_path_buf().try_into().unwrap();
                    self.save_state = SaveState::Idle;
                }
                Task::none()
            }
            SettingsMessage::AddDirectory => {
                // Indicate we're adding a new directory
                // This triggers the directory editor to show with default values
                self.editing_directory = EditState::Adding;
                let (directory_settings_form, initialize) =
                    DirectorySettingsForm::new(None, self.document_provider.clone());
                self.directory_settings_form = Some(directory_settings_form);
                initialize.map(ActionExt::map_into)
            }
            SettingsMessage::EditDirectory(path) => {
                // Load existing directory settings into the editor
                self.editing_directory = EditState::Editing(path.clone());
                let (expanded_path, dir_settings) = if let Ok(expanded_path) =
                    ExpandedPath::try_from(path.clone())
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
                    match &self.editing_directory {
                        EditState::Adding => {
                            // Adding a new directory - just insert it
                            self.settings
                                .scan
                                .directories
                                .insert(expanded_path, dir_settings);
                        }
                        EditState::Editing(original_path) => {
                            // Editing an existing directory - remove old entry and add new one
                            if let Ok(expanded_original) =
                                ExpandedPath::try_from(original_path.clone())
                            {
                                self.settings.scan.directories.remove(&expanded_original);
                            }
                            self.settings
                                .scan
                                .directories
                                .insert(expanded_path, dir_settings);
                        }
                        _ => {}
                    }

                    self.editing_directory = EditState::Idle;
                    self.directory_settings_form = None;
                    self.save_state = SaveState::Idle;
                    Task::none()
                }
            }
            SettingsMessage::CancelEditDirectory => {
                // Cancel directory editing and reset the editor state
                self.editing_directory = EditState::Idle;
                self.directory_settings_form = None;
                Task::none()
            }
            SettingsMessage::RemoveDirectory(path) => {
                // Remove a directory from the scan settings
                if let Ok(expanded_path) = ExpandedPath::try_from(path) {
                    self.settings.scan.directories.remove(&expanded_path);
                    self.save_state = SaveState::Idle;
                }
                Task::none()
            }
            SettingsMessage::Save => {
                self.save_state = SaveState::Saving;
                let settings = self.settings.clone();
                let config_path = self.application_module.config_path().to_owned();
                task::future(async move {
                    match settings.save(&config_path) {
                        Ok(()) => SettingsMessage::SaveComplete,
                        Err(e) => SettingsMessage::SaveError(e.to_string()),
                    }
                })
            }
            SettingsMessage::SaveComplete => {
                self.save_state = SaveState::Saved;
                // Update original settings to reflect saved state
                self.original_settings = Arc::new(self.settings.clone());
                // Invalidate application module to refresh the documentation in the rest of the app
                let am = Arc::clone(&self.application_module);
                task::future(async move {
                    am.set_expired().await;
                    SettingsMessage::Noop
                })
            }
            SettingsMessage::SaveError(error) => {
                self.save_state = SaveState::Error(error);
                Task::none()
            }
            SettingsMessage::AddAuthorizedUser => {
                let (authorized_user_form, init_authorized_user_form) =
                    AuthorizedUserForm::new(None);
                self.authorized_user_form = Some(authorized_user_form);
                init_authorized_user_form.map(ActionExt::map_into)
            }
            SettingsMessage::DeleteAuthorizedUser(user_id) => {
                self.settings.server.authorized_users.shift_remove(&user_id);
                if self.is_editing_authorized_user(&user_id) {
                    self.authorized_user_form = None;
                }
                Task::none()
            }
            SettingsMessage::EditAuthorizedUser(user_id) => {
                if !self.settings.server.authorized_users.contains_key(&user_id) {
                    return Task::none();
                };

                let (authorized_user_form, init_authorized_user_form) =
                    AuthorizedUserForm::new(Some(user_id));
                self.authorized_user_form = Some(authorized_user_form);
                init_authorized_user_form.map(ActionExt::map_into)
            }
            SettingsMessage::AuthorizedUserForm(message) => match message {
                AuthorizedUserFormMessage::Out(message) => {
                    match message {
                        AuthorizedUserFormOutput::Submit(
                            Some(original_user_id),
                            user_id,
                            passphrase,
                        ) => {
                            let authorized_users = &mut self.settings.server.authorized_users;

                            if original_user_id != user_id {
                                authorized_users.shift_remove(&original_user_id);
                                // TODO: error handling
                                authorized_users.insert(user_id, passphrase.try_into().unwrap());
                            } else if let Some(value) = authorized_users.get_mut(&user_id) {
                                *value = passphrase.try_into().unwrap(); // TODO: error handling
                            }
                        }
                        AuthorizedUserFormOutput::Submit(None, user_id, passphrase) => {
                            self.settings
                                .server
                                .authorized_users
                                .insert(user_id, passphrase.try_into().unwrap());
                        }
                        AuthorizedUserFormOutput::Cancel => {
                            // Nothing to do, form will be deleted
                        }
                    };
                    self.authorized_user_form = None;
                    task::none()
                }
                _ => match self.authorized_user_form.as_mut() {
                    Some(form) => form.update(message).map(ActionExt::map_into),
                    None => task::none(),
                },
            },
            SettingsMessage::ToggleDocumentType(doc_type, enabled) => {
                if enabled {
                    if !self.settings.scan.extensions.contains(&doc_type) {
                        self.settings.scan.extensions.push(doc_type);
                        self.settings.scan.extensions.sort();
                    }
                } else {
                    self.settings.scan.extensions.retain(|x| x != &doc_type);
                }
                self.save_state = SaveState::Idle;
                Task::none()
            }
            SettingsMessage::SectionChanged(section) => {
                self.selected_section = section;
                self.directory_settings_form = None;
                self.editing_directory = EditState::Idle;
                self.authorized_user_form = None;
                Task::none()
            }
            SettingsMessage::Noop => task::none(),
        }
    }
}

fn view_directory<'a>(
    path: &'a ExpandedPath,
    dir_settings: &'a DirectorySettings,
) -> widget::Row<'a, SettingsMessage, Theme> {
    let cosmic_theme::Spacing { space_s, .. } = theme::active().cosmic().spacing;

    let action = match dir_settings {
        read_flow_core::scan::DirectorySettings::Ignore { .. } => {
            fl!("settings-directory-action-ignore")
        }
        read_flow_core::scan::DirectorySettings::Scan { .. } => {
            fl!("settings-directory-action-scan")
        }
    };

    let edit_button = widget::button::icon(icon::from_name("edit-symbolic").size(ICON_SIZE))
        .on_press(SettingsMessage::EditDirectory(path.clone().into()))
        .tooltip(fl!("settings-edit-directory"));

    let remove_button =
        widget::button::icon(icon::from_name("list-remove-symbolic").size(ICON_SIZE))
            .class(widget::button::ButtonClass::Destructive)
            .on_press(SettingsMessage::RemoveDirectory(path.clone().into()))
            .tooltip(fl!("settings-remove-directory"));

    let controls = widget::Row::new()
        .push(edit_button)
        .push(remove_button)
        .spacing(space_s)
        .apply(container)
        .align_right(Length::Shrink);

    widget::settings::item_row(vec![
        widget::settings::item_row(vec![
            widget::icon::from_name("folder-symbolic")
                .size(ICON_SIZE)
                .apply(widget::container)
                .into(),
            widget::text::monotext(path.display().to_string())
                .width(Length::Fill)
                .into(),
        ])
        .width(Length::FillPortion(4))
        .into(),
        widget::text::body(action)
            .width(Length::FillPortion(1))
            .into(),
        controls.width(Length::FillPortion(1)).into(),
    ])
}
