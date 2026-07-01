// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use cosmic::Action;
use cosmic::Application;
use cosmic::Apply;
use cosmic::Element;
use cosmic::Task;
use cosmic::Theme;
use cosmic::cosmic_config;
use cosmic::cosmic_config::CosmicConfigEntry;
use cosmic::cosmic_theme;
use cosmic::iced::Length;
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::alignment::Vertical;
use cosmic::task;
use cosmic::theme;
use cosmic::widget;
use cosmic::widget::container;
use cosmic::widget::icon;
use cosmic::widget::settings;
use provider::r#async::HasSetExpired;
use provider::r#async::Provider;
use read_flow_core::Builder;
use read_flow_core::ExpandedPath;
use read_flow_core::api::FileDataSource;
use read_flow_core::client::FilesClient;
use read_flow_core::db;
use read_flow_core::db::dao;
use read_flow_core::db::models::NewRemote;
use read_flow_core::db::models::Remote;
use read_flow_core::scan::DirectorySettings;
use read_flow_core::scan::DocumentType;
use read_flow_core::settings::Settings;
use read_flow_core::settings::TlsSettings;
use read_flow_core::settings::UserEntry;
use rfd::AsyncFileDialog;
use rfd::FileHandle;
use url::Url;

use crate::ApplicationModule;
use crate::ICON_SIZE;
use crate::app::ContextView;
use crate::component::provided_state::ProvidedState;
use crate::component::provided_state::ProvidedStateMessage;
use crate::component::tag_editor::Orientation;
use crate::component::tag_editor::TagEditor;
use crate::component::tag_editor::TagEditorMessage;
use crate::component::tag_editor::TagEditorOutput;
use crate::config::Config;
use crate::config::EpubViewerConfig;
use crate::cosmic_ext::ActionExt;
use crate::document_provider::DocumentProvider;
use crate::fl;
use crate::forms::settings::authorized_user::AuthorizedUserForm;
use crate::forms::settings::authorized_user::AuthorizedUserFormMessage;
use crate::forms::settings::authorized_user::AuthorizedUserFormOutput;
use crate::forms::settings::directory_settings::DirectorySettingsForm;
use crate::forms::settings::directory_settings::DirectorySettingsFormMessage;
use crate::forms::settings::directory_settings::DirectorySettingsFormOutput;
use crate::forms::sources::add_source::AddSourceForm;
use crate::forms::sources::add_source::AddSourceFormMessage;
use crate::forms::sources::add_source::AddSourceFormOutput;
use crate::iter::find_with_next;
use crate::iter::find_with_previous;
use crate::layout::layout;
use crate::page::Page;
use crate::state::LoadedState;

#[derive(Debug, Clone)]
struct RemotesProvider(Arc<ApplicationModule>);

impl Provider<Vec<Remote>> for RemotesProvider {
    type Error = db::dao::Error;

    async fn provide(&self) -> Result<Vec<Remote>, Self::Error> {
        let pool = self.0.connection_pool().await;
        let mut conn = pool.acquire().await?;
        dao::select_all_remotes(&mut conn).await
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EditState<T> {
    Idle,
    Adding,
    Editing(T),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveState {
    Idle,
    Saving,
    Saved,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PreferencesSection {
    #[default]
    Overview,
    Appearance,
    Library,
    Downloads,
    Scanning,
    Sources,
    Server,
    Privacy,
}

pub struct PreferencesPage {
    // settings (TOML-backed)
    application_module: Arc<ApplicationModule>,
    document_provider: Arc<DocumentProvider>,
    original_settings: Arc<Settings>,
    settings: Settings,
    tag_editor: TagEditor<Arc<DocumentProvider>>,
    save_state: SaveState,
    editing_directory: EditState<PathBuf>,
    directory_settings_form: Option<DirectorySettingsForm>,
    authorized_user_form: Option<AuthorizedUserForm>,
    // appearance (cosmic_config-backed)
    config: Config,
    // sources (DB-backed)
    remotes_state: ProvidedState<RemotesProvider, Vec<Remote>>,
    add_source_form: Option<AddSourceForm>,
    operation_error: Option<String>,
    pending_deletion: Option<Remote>,
    source_statuses: HashMap<i32, LoadedState<bool>>,
    // section nav
    selected_section: PreferencesSection,
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone)]
pub enum PreferencesOutput {
    SourceAdded(Url, String, String),
    SourceEdited(Url, Url, String, String),
    SourceDeleted(Url),
    RestartServer,
}

#[derive(Debug, Clone)]
pub enum PreferencesMessage {
    SectionChanged(PreferencesSection),

    // appearance
    SetEpubViewer(EpubViewerConfig),

    // settings (TOML)
    ToggleDryRun(bool),
    TogglePrivateMode(bool),
    TagEditor(TagEditorMessage),
    DirectorySettingsForm(DirectorySettingsFormMessage),
    SelectDatabaseLocation,
    SelectedDatabaseLocation(Option<FileHandle>),
    SelectClientDownloadFolder,
    SelectedClientDownloadFolder(Option<FileHandle>),
    SelectServerDownloadFolder,
    SelectedServerDownloadFolder(Option<FileHandle>),
    ServerAddressChanged(String),
    ServerPortChanged(String),
    ServerAllowedOriginsChanged(String),
    ServerMaxUploadChanged(String),
    ToggleServerTls(bool),
    ServerTlsCertChanged(String),
    ServerTlsKeyChanged(String),
    GenerateTlsCert,
    GeneratedTlsCert(Result<(PathBuf, PathBuf), String>),
    ToggleServerStartOnLaunch(bool),
    AuthorizedUserForm(AuthorizedUserFormMessage),
    AddAuthorizedUser,
    EditAuthorizedUser(String),
    DeleteAuthorizedUser(String),
    Save,
    SaveComplete,
    SaveError(String),
    ToggleDocumentType(DocumentType, bool),
    ToggleAllDocumentTypes(bool),
    AddDirectory,
    RemoveDirectory(PathBuf),
    EditDirectory(PathBuf),
    SaveDirectory(ExpandedPath, DirectorySettings),
    CancelEditDirectory,

    // sources (DB)
    Remotes(ProvidedStateMessage<Vec<Remote>>),
    ShowAddSourceForm,
    EditSource(Remote),
    AddSourceForm(AddSourceFormMessage),
    SubmitSource(Option<Remote>, Url, String, String),
    SubmittedSource(Option<Remote>, Url, String, String),
    RequestDeleteSource(Remote),
    ConfirmDeleteSource,
    CancelDeleteSource,
    DeleteSource(i32),
    DeletedSource(i32),
    SetOperationError(String),
    ClearOperationError,
    MoveSourceUp(Remote),
    MoveSourceDown(Remote),
    SwapOrderOfRemotes(Remote, Remote),
    CheckSourceStatus(Remote),
    SetSourceStatus(i32, bool),
    RefreshStatuses,

    Out(PreferencesOutput),
    Noop,
}

impl From<TagEditorMessage> for PreferencesMessage {
    fn from(v: TagEditorMessage) -> Self {
        Self::TagEditor(v)
    }
}

impl From<DirectorySettingsFormMessage> for PreferencesMessage {
    fn from(v: DirectorySettingsFormMessage) -> Self {
        Self::DirectorySettingsForm(v)
    }
}

impl From<AuthorizedUserFormMessage> for PreferencesMessage {
    fn from(v: AuthorizedUserFormMessage) -> Self {
        Self::AuthorizedUserForm(v)
    }
}

impl From<ProvidedStateMessage<Vec<Remote>>> for PreferencesMessage {
    fn from(v: ProvidedStateMessage<Vec<Remote>>) -> Self {
        Self::Remotes(v)
    }
}

impl From<AddSourceFormMessage> for PreferencesMessage {
    fn from(v: AddSourceFormMessage) -> Self {
        Self::AddSourceForm(v)
    }
}

impl PreferencesPage {
    pub fn new(
        application_module: Arc<ApplicationModule>,
        config: Config,
        document_provider: Arc<DocumentProvider>,
    ) -> (Self, Task<Action<PreferencesMessage>>) {
        let settings: Arc<Settings> = Arc::new(
            Settings::extract_from(application_module.config_path()).expect("settings are present"),
        );

        let (tag_editor, tag_editor_task) = TagEditor::new(
            document_provider.clone(),
            settings.ui.private_tags().to_vec(),
            Orientation::Vertical,
            fl!("settings-select-private-tag"),
            fl!("settings-no-private-tags"),
            fl!("settings-remove-private-tag"),
        );

        let (remotes_state, init_remotes) =
            ProvidedState::new(RemotesProvider(application_module.clone()));

        let tasks = task::batch([
            tag_editor_task.map(ActionExt::map_into),
            init_remotes.map(ActionExt::map_into),
        ]);

        (
            Self {
                application_module,
                document_provider,
                original_settings: settings.clone(),
                settings: (*settings).clone(),
                tag_editor,
                save_state: SaveState::Idle,
                editing_directory: EditState::Idle,
                directory_settings_form: None,
                authorized_user_form: None,
                config,
                remotes_state,
                add_source_form: None,
                operation_error: None,
                pending_deletion: None,
                source_statuses: HashMap::new(),
                selected_section: PreferencesSection::default(),
            },
            tasks,
        )
    }

    pub fn update_config(&mut self, config: Config) {
        self.config = config;
    }

    #[cfg(test)]
    pub fn epub_viewer(&self) -> EpubViewerConfig {
        self.config.epub_viewer
    }

    pub fn current_private_mode(&self) -> bool {
        self.settings.ui.private_mode()
    }

    fn is_modified(&self) -> bool {
        self.settings != *self.original_settings
    }

    fn can_be_saved(&self) -> bool {
        self.authorized_user_form.is_none() && self.directory_settings_form.is_none()
    }

    fn view_save_row(&self) -> Element<'_, PreferencesMessage> {
        let cosmic_theme::Spacing {
            space_s, space_m, ..
        } = theme::active().cosmic().spacing;

        let save_button = if self.is_modified() && self.can_be_saved() {
            widget::button::suggested(fl!("settings-save")).on_press(PreferencesMessage::Save)
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

    fn view_overview(&self) -> Vec<Element<'_, PreferencesMessage>> {
        [
            (
                PreferencesSection::Appearance,
                fl!("preferences-appearance-section"),
                fl!("preferences-appearance-section-description"),
                "application-epub+zip",
            ),
            (
                PreferencesSection::Library,
                fl!("preferences-library-section"),
                fl!("preferences-library-section-description"),
                "package-x-generic-symbolic",
            ),
            (
                PreferencesSection::Downloads,
                fl!("preferences-downloads-section"),
                fl!("preferences-downloads-section-description"),
                "folder-download-symbolic",
            ),
            (
                PreferencesSection::Scanning,
                fl!("preferences-scanning-section"),
                fl!("preferences-scanning-section-description"),
                "system-search-symbolic",
            ),
            (
                PreferencesSection::Sources,
                fl!("preferences-sources-section"),
                fl!("preferences-sources-section-description"),
                "network-server-symbolic",
            ),
            (
                PreferencesSection::Server,
                fl!("preferences-server-section"),
                fl!("preferences-server-section-description"),
                "network-server-symbolic",
            ),
            (
                PreferencesSection::Privacy,
                fl!("preferences-privacy-section"),
                fl!("preferences-privacy-section-description"),
                "preferences-system-privacy-symbolic",
            ),
        ]
        .into_iter()
        .map(|(section, label, description, icon_name)| {
            widget::settings::section()
                .add(
                    widget::settings::item::builder(label)
                        .description(description)
                        .icon(widget::icon::from_name(icon_name).size(ICON_SIZE))
                        .control(widget::icon::from_name("go-next-symbolic").size(ICON_SIZE))
                        .apply(widget::mouse_area)
                        .on_press(PreferencesMessage::SectionChanged(section)),
                )
                .into()
        })
        .collect()
    }

    fn view_back_button(&self) -> Element<'_, PreferencesMessage> {
        widget::Row::new()
            .push(widget::button::link(fl!("preferences-back")).on_press(
                PreferencesMessage::SectionChanged(PreferencesSection::Overview),
            ))
            .into()
    }

    fn view_section_appearance(&self) -> Vec<Element<'_, PreferencesMessage>> {
        let cosmic_theme::Spacing { space_xs, .. } = theme::active().cosmic().spacing;

        let viewer_section = widget::settings::section()
            .title(fl!("settings-viewer-section"))
            .add(
                widget::settings::item::builder(fl!("settings-epub-viewer"))
                    .description(fl!("settings-epub-viewer-description"))
                    .icon(widget::icon::from_name("application-epub+zip").size(ICON_SIZE))
                    .control(
                        widget::Column::from_vec(vec![
                            widget::radio(
                                widget::text::body(fl!("settings-epub-viewer-native")),
                                EpubViewerConfig::NativeEpub,
                                Some(self.config.epub_viewer),
                                PreferencesMessage::SetEpubViewer,
                            )
                            .into(),
                            widget::radio(
                                widget::text::body(fl!("settings-epub-viewer-mupdf")),
                                EpubViewerConfig::MuPdf,
                                Some(self.config.epub_viewer),
                                PreferencesMessage::SetEpubViewer,
                            )
                            .into(),
                            widget::radio(
                                widget::text::body(fl!("settings-epub-viewer-external")),
                                EpubViewerConfig::ExternalViewer,
                                Some(self.config.epub_viewer),
                                PreferencesMessage::SetEpubViewer,
                            )
                            .into(),
                        ])
                        .spacing(space_xs)
                        .align_x(Horizontal::Left),
                    ),
            );

        vec![
            widget::text::title2(fl!("preferences-appearance-section")).into(),
            viewer_section.into(),
        ]
    }

    fn view_section_library(&self) -> Vec<Element<'_, PreferencesMessage>> {
        let database_section = widget::settings::section()
            .title(fl!("preferences-library-section"))
            .add(
                widget::settings::item::builder(fl!("settings-database-location"))
                    .description(fl!("settings-database-location-description"))
                    .icon(widget::icon::from_name("package-x-generic-symbolic").size(ICON_SIZE))
                    .control(
                        widget::settings::item_row(vec![
                            widget::text::monotext(self.settings.database.url().to_string()).into(),
                            widget::button::text("Select")
                                .on_press(PreferencesMessage::SelectDatabaseLocation)
                                .into(),
                        ])
                        .align_y(Vertical::Center)
                        .width(Length::Shrink),
                    ),
            );

        vec![
            widget::text::title2(fl!("preferences-library-section")).into(),
            database_section.into(),
        ]
    }

    fn view_section_downloads(&self) -> Vec<Element<'_, PreferencesMessage>> {
        let client_section = widget::settings::section()
            .title(fl!("preferences-downloads-section"))
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
                                .on_press(PreferencesMessage::SelectClientDownloadFolder)
                                .into(),
                        ])
                        .align_y(Vertical::Center)
                        .width(Length::Shrink),
                    ),
            );

        vec![
            widget::text::title2(fl!("preferences-downloads-section")).into(),
            client_section.into(),
        ]
    }

    fn view_section_scanning(&self) -> Vec<Element<'_, PreferencesMessage>> {
        let scan_section = widget::settings::section()
            .title(fl!("preferences-scanning-section"))
            .add(widget::text::body(fl!("settings-scan-description")).width(Length::Fill))
            .add(
                widget::settings::item::builder(fl!("settings-scan-dry-run"))
                    .description(fl!("settings-scan-dry-run-description"))
                    .icon(widget::icon::from_name("system-run-symbolic").size(ICON_SIZE))
                    .toggler(self.settings.scan.dry_run, PreferencesMessage::ToggleDryRun),
            );

        let all_selected = DocumentType::all()
            .iter()
            .all(|t| self.settings.scan.extensions.contains(t));
        let toggle_all_button = widget::button::text(if all_selected {
            fl!("settings-scan-file-types-deselect-all")
        } else {
            fl!("settings-scan-file-types-select-all")
        })
        .on_press(PreferencesMessage::ToggleAllDocumentTypes(!all_selected));

        let file_types_section = DocumentType::all().iter().fold(
            widget::settings::section()
                .header(widget::settings::item_row(vec![
                    widget::text::heading(fl!("settings-scan-file-types-section")).into(),
                    widget::space::horizontal().into(),
                    toggle_all_button.into(),
                ]))
                .add(
                    widget::text::body(fl!("settings-scan-file-types-description"))
                        .width(Length::Fill),
                ),
            |section, doc_type| {
                let enabled = self.settings.scan.extensions.contains(doc_type);
                let doc_type_clone = *doc_type;
                section.add(
                    widget::settings::item::builder(format!(".{}", doc_type.as_str()))
                        .description(doc_type.label())
                        .toggler(enabled, move |v| {
                            PreferencesMessage::ToggleDocumentType(doc_type_clone, v)
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
            .add(crate::component::section_helpers::section_add_button(
                fl!("settings-add-directory"),
                Some(PreferencesMessage::AddDirectory),
            ));

        let mut items: Vec<Element<'_, PreferencesMessage>> = vec![
            widget::text::title2(fl!("preferences-scanning-section")).into(),
            scan_section.into(),
            directories_section.into(),
        ];

        if let Some(form) = self.directory_settings_form.as_ref() {
            items.push(form.view().map(Into::into));
        }

        items.push(file_types_section.into());
        items
    }

    fn view_section_sources(&self) -> Vec<Element<'_, PreferencesMessage>> {
        let sources_section = match &self.remotes_state.state {
            LoadedState::New => settings::section()
                .title(fl!("sources-section-title"))
                .add(widget::text(fl!("sources-loading-state-new")))
                .into(),
            LoadedState::Loading => settings::section()
                .title(fl!("sources-section-title"))
                .add(widget::text(fl!("sources-loading-state-loading")))
                .into(),
            LoadedState::Failed(error) => settings::section()
                .title(fl!("sources-section-title"))
                .add(widget::text(fl!("generic-error", error = error)))
                .into(),
            LoadedState::Loaded(sources) => {
                let section = if sources.is_empty() {
                    settings::section()
                        .title(fl!("sources-section-title"))
                        .add(widget::text(fl!("sources-empty-state")))
                } else {
                    sources.iter().enumerate().fold(
                        settings::section().title(fl!("sources-section-title")),
                        |section, (index, source)| {
                            section.add(self.view_source(
                                source,
                                index == 0,
                                index == sources.len() - 1,
                            ))
                        },
                    )
                };
                section
                    .add(crate::component::section_helpers::section_add_button(
                        fl!("sources-add-button"),
                        self.add_source_form
                            .is_none()
                            .then_some(PreferencesMessage::ShowAddSourceForm),
                    ))
                    .into()
            }
        };

        let mut items: Vec<Element<'_, PreferencesMessage>> = vec![
            widget::text::title2(fl!("preferences-sources-section")).into(),
            sources_section,
        ];

        if let Some(form) = &self.add_source_form {
            items.push(form.view().map(PreferencesMessage::AddSourceForm));
        }

        items
    }

    fn view_source<'a>(
        &self,
        source: &'a Remote,
        is_first: bool,
        is_last: bool,
    ) -> Element<'a, PreferencesMessage> {
        let (status_icon_name, status_tooltip) = match self.source_statuses.get(&source.id) {
            None | Some(LoadedState::New) => {
                ("dialog-question-symbolic", fl!("sources-status-unknown"))
            }
            Some(LoadedState::Loading) => (
                "emblem-synchronizing-symbolic",
                fl!("sources-status-checking"),
            ),
            Some(LoadedState::Loaded(true)) => {
                ("emblem-ok-symbolic", fl!("sources-status-reachable"))
            }
            Some(LoadedState::Loaded(false)) | Some(LoadedState::Failed(_)) => (
                "network-offline-symbolic",
                fl!("sources-status-unreachable"),
            ),
        };
        let status_icon = widget::tooltip::tooltip(
            icon::from_name(status_icon_name).size(ICON_SIZE),
            widget::text(status_tooltip),
            widget::tooltip::Position::Bottom,
        );
        widget::settings::item::builder(&source.base_url)
            .icon(icon::from_name("network-server-symbolic").size(ICON_SIZE))
            .control(
                widget::settings::item_row(vec![
                    status_icon.into(),
                    widget::button::icon(icon::from_name("go-up-symbolic").size(ICON_SIZE))
                        .class(theme::Button::Icon)
                        .apply_if(!is_first, |button| {
                            button.on_press(PreferencesMessage::MoveSourceUp(source.clone()))
                        })
                        .into(),
                    widget::button::icon(icon::from_name("go-down-symbolic").size(ICON_SIZE))
                        .class(theme::Button::Icon)
                        .apply_if(!is_last, |button| {
                            button.on_press(PreferencesMessage::MoveSourceDown(source.clone()))
                        })
                        .into(),
                    widget::button::icon(icon::from_name("edit-symbolic").size(ICON_SIZE))
                        .class(theme::Button::Icon)
                        .on_press(PreferencesMessage::EditSource(source.clone()))
                        .into(),
                    widget::button::icon(icon::from_name("list-remove-symbolic").size(ICON_SIZE))
                        .class(theme::Button::Destructive)
                        .on_press(PreferencesMessage::RequestDeleteSource(source.clone()))
                        .into(),
                ])
                .width(Length::Shrink),
            )
            .into()
    }

    /// Get the TLS settings, creating an empty entry (so cert/key can be typed
    /// in) if none exists.
    fn ensure_tls(&mut self) -> &mut TlsSettings {
        self.settings.server.tls.get_or_insert_with(|| TlsSettings {
            cert: ExpandedPath::from_str("").expect("empty path"),
            key: ExpandedPath::from_str("").expect("empty path"),
        })
    }

    fn view_section_server(&self) -> Vec<Element<'_, PreferencesMessage>> {
        let server_section = widget::settings::section()
            .title(fl!("preferences-server-section"))
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
                                .on_press(PreferencesMessage::SelectServerDownloadFolder)
                                .into(),
                        ])
                        .align_y(Vertical::Center)
                        .width(Length::Shrink),
                    ),
            )
            .add(
                widget::settings::item::builder(fl!("settings-server-address"))
                    .description(fl!("settings-server-address-description"))
                    .control(
                        widget::text_input(
                            "127.0.0.1",
                            self.settings.server.address.clone().unwrap_or_default(),
                        )
                        .on_input(PreferencesMessage::ServerAddressChanged)
                        .width(Length::Fixed(180.0)),
                    ),
            )
            .add(
                widget::settings::item::builder(fl!("settings-server-port"))
                    .description(fl!("settings-server-port-description"))
                    .control(
                        widget::text_input(
                            "8000",
                            self.settings
                                .server
                                .port
                                .map(|p| p.to_string())
                                .unwrap_or_default(),
                        )
                        .on_input(PreferencesMessage::ServerPortChanged)
                        .width(Length::Fixed(120.0)),
                    ),
            )
            .add(
                widget::settings::item::builder(fl!("settings-server-allowed-origins"))
                    .description(fl!("settings-server-allowed-origins-description"))
                    .control(
                        widget::text_input(
                            fl!("settings-server-allowed-origins-placeholder"),
                            self.settings.server.allowed_origins.join(", "),
                        )
                        .on_input(PreferencesMessage::ServerAllowedOriginsChanged)
                        .width(Length::Fixed(240.0)),
                    ),
            )
            .add(
                widget::settings::item::builder(fl!("settings-server-max-upload"))
                    .description(fl!("settings-server-max-upload-description"))
                    .control(
                        widget::text_input(
                            "100",
                            self.settings
                                .server
                                .max_upload_bytes
                                .map(|b| (b / (1024 * 1024)).to_string())
                                .unwrap_or_default(),
                        )
                        .on_input(PreferencesMessage::ServerMaxUploadChanged)
                        .width(Length::Fixed(120.0)),
                    ),
            )
            .add(
                widget::settings::item::builder(fl!("settings-server-tls"))
                    .description(fl!("settings-server-tls-description"))
                    .toggler(
                        self.settings.server.tls.is_some(),
                        PreferencesMessage::ToggleServerTls,
                    ),
            )
            .add(
                widget::settings::item::builder(fl!("settings-server-tls-cert")).control(
                    widget::text_input(
                        fl!("settings-server-tls-cert-placeholder"),
                        self.settings
                            .server
                            .tls
                            .as_ref()
                            .map(|t| t.cert.display().to_string())
                            .unwrap_or_default(),
                    )
                    .on_input(PreferencesMessage::ServerTlsCertChanged)
                    .width(Length::Fixed(240.0)),
                ),
            )
            .add(
                widget::settings::item::builder(fl!("settings-server-tls-key")).control(
                    widget::text_input(
                        fl!("settings-server-tls-key-placeholder"),
                        self.settings
                            .server
                            .tls
                            .as_ref()
                            .map(|t| t.key.display().to_string())
                            .unwrap_or_default(),
                    )
                    .on_input(PreferencesMessage::ServerTlsKeyChanged)
                    .width(Length::Fixed(240.0)),
                ),
            )
            .add(
                widget::settings::item::builder(fl!("settings-server-generate-cert"))
                    .description(fl!("settings-server-generate-cert-description"))
                    .control(
                        widget::button::standard(fl!("settings-server-generate-cert-button"))
                            .on_press(PreferencesMessage::GenerateTlsCert),
                    ),
            )
            .add(
                widget::settings::item::builder(fl!("settings-server-start-on-launch"))
                    .description(fl!("settings-server-start-on-launch-description"))
                    .toggler(
                        self.config.server_start_on_launch,
                        PreferencesMessage::ToggleServerStartOnLaunch,
                    ),
            )
            .add(
                widget::settings::item::builder(fl!("settings-server-restart-to-apply")).control(
                    widget::button::standard(fl!("server-restart"))
                        .on_press(PreferencesMessage::Out(PreferencesOutput::RestartServer)),
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
                |acc, (_index, (user_id, entry))| {
                    acc.add(self.view_authorized_user_input(user_id, entry))
                },
            )
            .add(crate::component::section_helpers::section_add_button(
                fl!("settings-server-add-authorized-user"),
                Some(PreferencesMessage::AddAuthorizedUser),
            ));

        let mut items: Vec<Element<'_, PreferencesMessage>> = vec![
            widget::text::title2(fl!("preferences-server-section")).into(),
            server_section.into(),
            authorized_users_section.into(),
        ];

        if let Some(form) = self.authorized_user_form.as_ref() {
            items.push(form.view().map(Into::into));
        }

        items
    }

    fn view_section_privacy(&self) -> Vec<Element<'_, PreferencesMessage>> {
        let privacy_section = widget::settings::section()
            .title(fl!("preferences-privacy-section"))
            .add(
                widget::settings::item::builder(fl!("settings-ui-private-mode"))
                    .description(fl!("settings-ui-private-mode-description"))
                    .icon(
                        widget::icon::from_name("preferences-system-privacy-symbolic")
                            .size(ICON_SIZE),
                    )
                    .toggler(
                        self.settings.ui.private_mode(),
                        PreferencesMessage::TogglePrivateMode,
                    ),
            )
            .add(
                widget::settings::item::builder(fl!("settings-ui-private-tags"))
                    .description(fl!("settings-ui-private-tags-description"))
                    .icon(widget::icon::from_name("starred-symbolic").size(ICON_SIZE))
                    .flex_control(self.tag_editor.view().map(PreferencesMessage::TagEditor)),
            );

        vec![
            widget::text::title2(fl!("preferences-privacy-section")).into(),
            privacy_section.into(),
        ]
    }

    fn view_authorized_user_input<'a>(
        &'a self,
        user_id: &'a String,
        entry: &'a UserEntry,
    ) -> Element<'a, PreferencesMessage> {
        let is_editing = self.is_editing_authorized_user(user_id);
        let icon_name = if entry.has_role("owner") {
            "security-high-symbolic"
        } else {
            "avatar-default-symbolic"
        };

        widget::settings::item::builder(user_id)
            .icon(widget::icon::from_name(icon_name).size(ICON_SIZE))
            .control(widget::settings::item_row(vec![
                widget::text_input("", format!("{}", entry.password())).into(),
                widget::button::icon(
                    widget::icon::from_name(if is_editing {
                        "edit-clear-symbolic"
                    } else {
                        "edit-symbolic"
                    })
                    .size(ICON_SIZE),
                )
                .apply_if(!is_editing, |button| {
                    button.on_press(PreferencesMessage::EditAuthorizedUser(user_id.clone()))
                })
                .into(),
                widget::button::icon(icon::from_name("list-remove-symbolic").size(ICON_SIZE))
                    .on_press(PreferencesMessage::DeleteAuthorizedUser(user_id.clone()))
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
}

impl Page for PreferencesPage {
    type Message = PreferencesMessage;

    fn view(&self) -> Element<'_, PreferencesMessage> {
        let mut items: Vec<Element<'_, PreferencesMessage>> = match self.selected_section {
            PreferencesSection::Overview => {
                let mut col = self.view_overview();
                col.insert(
                    0,
                    widget::text::title2(fl!("preferences-page-title")).into(),
                );
                col
            }
            ref section => {
                let mut items = vec![self.view_back_button()];
                items.extend(match section {
                    PreferencesSection::Appearance => self.view_section_appearance(),
                    PreferencesSection::Library => self.view_section_library(),
                    PreferencesSection::Downloads => self.view_section_downloads(),
                    PreferencesSection::Scanning => self.view_section_scanning(),
                    PreferencesSection::Sources => self.view_section_sources(),
                    PreferencesSection::Server => self.view_section_server(),
                    PreferencesSection::Privacy => self.view_section_privacy(),
                    PreferencesSection::Overview => unreachable!(),
                });
                items
            }
        };

        let needs_save_row = matches!(
            self.selected_section,
            PreferencesSection::Library
                | PreferencesSection::Downloads
                | PreferencesSection::Scanning
                | PreferencesSection::Server
                | PreferencesSection::Privacy
        );
        if needs_save_row {
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

    fn view_context(&self) -> ContextView<'_, PreferencesMessage> {
        ContextView {
            title: fl!("preferences-page-title"),
            content: widget::text("").into(),
        }
    }

    fn dialog(&self) -> Option<Element<'_, PreferencesMessage>> {
        let cosmic_theme::Spacing { space_s, .. } = theme::active().cosmic().spacing;

        if let Some(remote) = &self.pending_deletion {
            return Some(crate::component::confirm_dialog::confirm_delete_dialog(
                fl!("sources-delete-confirm-title"),
                fl!("sources-delete-confirm-body"),
                &remote.base_url,
                fl!("sources-delete-confirm-delete"),
                fl!("sources-delete-confirm-cancel"),
                PreferencesMessage::ConfirmDeleteSource,
                PreferencesMessage::CancelDeleteSource,
            ));
        }

        if let Some(error) = &self.operation_error {
            return Some(
                widget::dialog()
                    .title(fl!("sources-error-title"))
                    .control(
                        widget::text::monotext(error)
                            .apply(widget::container)
                            .class(cosmic::theme::Container::Card)
                            .padding(space_s)
                            .width(Length::Fill),
                    )
                    .icon(icon::from_name("dialog-error-symbolic").size(64))
                    .primary_action(
                        widget::button::suggested(fl!("sources-error-close"))
                            .on_press(PreferencesMessage::ClearOperationError),
                    )
                    .into(),
            );
        }

        None
    }

    fn update(&mut self, message: PreferencesMessage) -> Task<Action<PreferencesMessage>> {
        tracing::debug!("received: {message:?}");
        match message {
            PreferencesMessage::SectionChanged(section) => {
                self.selected_section = section;
                self.directory_settings_form = None;
                self.editing_directory = EditState::Idle;
                self.authorized_user_form = None;
                self.add_source_form = None;
                Task::none()
            }
            PreferencesMessage::SetEpubViewer(epub_viewer) => {
                self.config.epub_viewer = epub_viewer;
                if let Ok(ctx) =
                    cosmic_config::Config::new(crate::app::ReadFlow::APP_ID, Config::VERSION)
                {
                    let _ = self.config.write_entry(&ctx);
                }
                Task::none()
            }
            PreferencesMessage::ToggleDryRun(value) => {
                self.settings.scan.set_dry_run(value);
                self.save_state = SaveState::Idle;
                Task::none()
            }
            PreferencesMessage::TogglePrivateMode(value) => {
                self.settings.ui.set_private_mode(value);
                self.save_state = SaveState::Idle;
                Task::none()
            }
            PreferencesMessage::ServerAddressChanged(value) => {
                let trimmed = value.trim();
                self.settings.server.address = (!trimmed.is_empty()).then(|| trimmed.to_string());
                self.save_state = SaveState::Idle;
                Task::none()
            }
            PreferencesMessage::ServerPortChanged(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    self.settings.server.port = None;
                    self.save_state = SaveState::Idle;
                } else if let Ok(port) = trimmed.parse::<u16>() {
                    self.settings.server.port = Some(port);
                    self.save_state = SaveState::Idle;
                }
                // Ignore non-numeric input (keeps the previous value).
                Task::none()
            }
            PreferencesMessage::ServerAllowedOriginsChanged(value) => {
                self.settings.server.allowed_origins = value
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                self.save_state = SaveState::Idle;
                Task::none()
            }
            PreferencesMessage::ServerMaxUploadChanged(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    self.settings.server.max_upload_bytes = None;
                    self.save_state = SaveState::Idle;
                } else if let Ok(mib) = trimmed.parse::<u64>() {
                    self.settings.server.max_upload_bytes = Some(mib * 1024 * 1024);
                    self.save_state = SaveState::Idle;
                }
                Task::none()
            }
            PreferencesMessage::ToggleServerTls(enabled) => {
                if enabled {
                    self.ensure_tls();
                } else {
                    self.settings.server.tls = None;
                }
                self.save_state = SaveState::Idle;
                Task::none()
            }
            PreferencesMessage::ServerTlsCertChanged(value) => {
                if let Ok(path) = ExpandedPath::from_str(value.trim()) {
                    self.ensure_tls().cert = path;
                    self.save_state = SaveState::Idle;
                }
                Task::none()
            }
            PreferencesMessage::ServerTlsKeyChanged(value) => {
                if let Ok(path) = ExpandedPath::from_str(value.trim()) {
                    self.ensure_tls().key = path;
                    self.save_state = SaveState::Idle;
                }
                Task::none()
            }
            PreferencesMessage::GenerateTlsCert => {
                let dir = self
                    .application_module
                    .config_path()
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("."));
                // Cover localhost plus the configured address if it's a real host.
                let mut sans = vec!["localhost".to_string()];
                if let Some(addr) = &self.settings.server.address
                    && !addr.is_empty()
                    && addr != "127.0.0.1"
                    && addr != "0.0.0.0"
                {
                    sans.push(addr.clone());
                }
                task::future(async move {
                    let result = tokio::task::spawn_blocking(move || {
                        read_flow_core::server::generate_self_signed_cert(&dir, sans)
                    })
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r.map_err(|e| e.to_string()));
                    PreferencesMessage::GeneratedTlsCert(result)
                })
            }
            PreferencesMessage::GeneratedTlsCert(result) => {
                match result {
                    Ok((cert, key)) => {
                        if let (Ok(cert), Ok(key)) = (
                            ExpandedPath::from_str(&cert.to_string_lossy()),
                            ExpandedPath::from_str(&key.to_string_lossy()),
                        ) {
                            self.settings.server.tls = Some(TlsSettings { cert, key });
                            self.save_state = SaveState::Idle;
                        }
                    }
                    Err(error) => {
                        self.save_state = SaveState::Error(error);
                    }
                }
                Task::none()
            }
            PreferencesMessage::ToggleServerStartOnLaunch(value) => {
                self.config.server_start_on_launch = value;
                if let Ok(ctx) =
                    cosmic_config::Config::new(crate::app::ReadFlow::APP_ID, Config::VERSION)
                {
                    let _ = self.config.write_entry(&ctx);
                }
                Task::none()
            }
            PreferencesMessage::TagEditor(message) => match message {
                TagEditorMessage::Out(message) => match message {
                    TagEditorOutput::TagsUpdated(tags) => {
                        self.settings.ui.set_private_tags(tags);
                        self.save_state = SaveState::Idle;
                        Task::none()
                    }
                    TagEditorOutput::TagAdded(_) | TagEditorOutput::TagRemoved(_) => Task::none(),
                },
                message => self.tag_editor.update(message).map(ActionExt::map_into),
            },
            PreferencesMessage::DirectorySettingsForm(message) => match message {
                DirectorySettingsFormMessage::Out(output) => match output {
                    DirectorySettingsFormOutput::Cancelled => {
                        task::message(PreferencesMessage::CancelEditDirectory)
                    }
                    DirectorySettingsFormOutput::Ok(expanded_path, directory_settings) => {
                        task::message(PreferencesMessage::SaveDirectory(
                            expanded_path,
                            directory_settings,
                        ))
                    }
                },
                message => {
                    if let Some(directory_form) = self.directory_settings_form.as_mut() {
                        directory_form.update(message).map(ActionExt::map_into)
                    } else {
                        Task::none()
                    }
                }
            },
            PreferencesMessage::SelectDatabaseLocation => {
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
                    PreferencesMessage::SelectedDatabaseLocation(directory)
                })
            }
            PreferencesMessage::SelectedDatabaseLocation(file_handle) => {
                if let Some(file) = file_handle {
                    self.settings
                        .database
                        .set_url(file.path().to_path_buf().try_into().unwrap());
                    self.save_state = SaveState::Idle;
                }
                Task::none()
            }
            PreferencesMessage::SelectClientDownloadFolder => {
                let download_folder = self.settings.client.download_folder.clone();
                task::future(async move {
                    let directory = AsyncFileDialog::new()
                        .set_directory(download_folder)
                        .set_can_create_directories(true)
                        .pick_folder()
                        .await;
                    PreferencesMessage::SelectedClientDownloadFolder(directory)
                })
            }
            PreferencesMessage::SelectedClientDownloadFolder(file_handle) => {
                if let Some(file) = file_handle {
                    self.settings.client.download_folder =
                        file.path().to_path_buf().try_into().unwrap();
                    self.save_state = SaveState::Idle;
                }
                Task::none()
            }
            PreferencesMessage::SelectServerDownloadFolder => {
                let download_folder = self.settings.server.download_folder.clone();
                task::future(async move {
                    let directory = AsyncFileDialog::new()
                        .set_directory(download_folder)
                        .set_can_create_directories(true)
                        .pick_folder()
                        .await;
                    PreferencesMessage::SelectedServerDownloadFolder(directory)
                })
            }
            PreferencesMessage::SelectedServerDownloadFolder(file_handle) => {
                if let Some(file) = file_handle {
                    self.settings.server.download_folder =
                        file.path().to_path_buf().try_into().unwrap();
                    self.save_state = SaveState::Idle;
                }
                Task::none()
            }
            PreferencesMessage::AddDirectory => {
                self.editing_directory = EditState::Adding;
                let (directory_settings_form, initialize) =
                    DirectorySettingsForm::new(None, self.document_provider.clone());
                self.directory_settings_form = Some(directory_settings_form);
                initialize.map(ActionExt::map_into)
            }
            PreferencesMessage::EditDirectory(path) => {
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
            PreferencesMessage::SaveDirectory(expanded_path, dir_settings) => {
                if expanded_path.as_os_str().is_empty() {
                    Task::none()
                } else {
                    match &self.editing_directory {
                        EditState::Adding => {
                            self.settings
                                .scan
                                .directories
                                .insert(expanded_path, dir_settings);
                        }
                        EditState::Editing(original_path) => {
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
            PreferencesMessage::CancelEditDirectory => {
                self.editing_directory = EditState::Idle;
                self.directory_settings_form = None;
                Task::none()
            }
            PreferencesMessage::RemoveDirectory(path) => {
                if let Ok(expanded_path) = ExpandedPath::try_from(path) {
                    self.settings.scan.directories.remove(&expanded_path);
                    self.save_state = SaveState::Idle;
                }
                Task::none()
            }
            PreferencesMessage::Save => {
                self.save_state = SaveState::Saving;
                let settings = self.settings.clone();
                let config_path = self.application_module.config_path().to_owned();
                task::future(async move {
                    match settings.save(&config_path) {
                        Ok(()) => PreferencesMessage::SaveComplete,
                        Err(e) => PreferencesMessage::SaveError(e.to_string()),
                    }
                })
            }
            PreferencesMessage::SaveComplete => {
                self.save_state = SaveState::Saved;
                self.original_settings = Arc::new(self.settings.clone());
                let am = Arc::clone(&self.application_module);
                task::future(async move {
                    am.set_expired().await;
                    PreferencesMessage::Noop
                })
            }
            PreferencesMessage::SaveError(error) => {
                self.save_state = SaveState::Error(error);
                Task::none()
            }
            PreferencesMessage::AddAuthorizedUser => {
                let (authorized_user_form, init) = AuthorizedUserForm::new(None, vec![]);
                self.authorized_user_form = Some(authorized_user_form);
                init.map(ActionExt::map_into)
            }
            PreferencesMessage::DeleteAuthorizedUser(user_id) => {
                self.settings.server.authorized_users.shift_remove(&user_id);
                if self.is_editing_authorized_user(&user_id) {
                    self.authorized_user_form = None;
                }
                Task::none()
            }
            PreferencesMessage::EditAuthorizedUser(user_id) => {
                let roles = self
                    .settings
                    .server
                    .authorized_users
                    .get(&user_id)
                    .map(|e| e.roles().to_vec())
                    .unwrap_or_default();
                let (authorized_user_form, init) = AuthorizedUserForm::new(Some(user_id), roles);
                self.authorized_user_form = Some(authorized_user_form);
                init.map(ActionExt::map_into)
            }
            PreferencesMessage::AuthorizedUserForm(message) => match message {
                AuthorizedUserFormMessage::Out(output) => {
                    match output {
                        AuthorizedUserFormOutput::Submit(
                            Some(original_user_id),
                            user_id,
                            passphrase,
                            roles,
                        ) => {
                            let authorized_users = &mut self.settings.server.authorized_users;
                            let entry = make_user_entry(passphrase, roles);
                            if original_user_id != user_id {
                                authorized_users.shift_remove(&original_user_id);
                                authorized_users.insert(user_id, entry);
                            } else if let Some(value) = authorized_users.get_mut(&user_id) {
                                *value = entry;
                            }
                        }
                        AuthorizedUserFormOutput::Submit(None, user_id, passphrase, roles) => {
                            self.settings
                                .server
                                .authorized_users
                                .insert(user_id, make_user_entry(passphrase, roles));
                        }
                        AuthorizedUserFormOutput::Cancel => {}
                    };
                    self.authorized_user_form = None;
                    task::none()
                }
                _ => match self.authorized_user_form.as_mut() {
                    Some(form) => form.update(message).map(ActionExt::map_into),
                    None => task::none(),
                },
            },
            PreferencesMessage::ToggleDocumentType(doc_type, enabled) => {
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
            PreferencesMessage::ToggleAllDocumentTypes(enabled) => {
                if enabled {
                    self.settings.scan.extensions = DocumentType::all().to_vec();
                } else {
                    self.settings.scan.extensions.clear();
                }
                self.save_state = SaveState::Idle;
                Task::none()
            }
            // Sources messages
            PreferencesMessage::ShowAddSourceForm => {
                let (form, task) = AddSourceForm::new(None);
                self.add_source_form = Some(form);
                task.map(ActionExt::map_into)
            }
            PreferencesMessage::EditSource(remote) => {
                let (form, task) = AddSourceForm::new(Some(&remote));
                self.add_source_form = Some(form);
                task.map(ActionExt::map_into)
            }
            PreferencesMessage::AddSourceForm(msg) => match msg {
                AddSourceFormMessage::Out(output) => match output {
                    AddSourceFormOutput::Cancel => {
                        self.add_source_form = None;
                        task::none()
                    }
                    AddSourceFormOutput::Submit(original, url, user_id, passphrase) => {
                        task::message(PreferencesMessage::SubmitSource(
                            original, *url, user_id, passphrase,
                        ))
                    }
                },
                msg => match &mut self.add_source_form {
                    Some(form) => form.update(msg).map(ActionExt::map_into),
                    None => task::none(),
                },
            },
            PreferencesMessage::Remotes(message) => {
                let task = self.remotes_state.update(message).map(ActionExt::map_into);
                if let LoadedState::Loaded(remotes) = &self.remotes_state.state {
                    let mut check_tasks: Vec<_> = remotes
                        .iter()
                        .map(|remote| {
                            task::message(PreferencesMessage::CheckSourceStatus(remote.clone()))
                        })
                        .collect();
                    check_tasks.push(task);
                    Task::batch(check_tasks)
                } else {
                    task
                }
            }
            PreferencesMessage::SubmitSource(original, url, user_id, passphrase) => {
                let am = Arc::clone(&self.application_module);
                task::future(async move {
                    let connection_pool = am.connection_pool().await;
                    let mut conn = match connection_pool.acquire().await {
                        Ok(conn) => conn,
                        Err(e) => return PreferencesMessage::SetOperationError(format!("{e}")),
                    };
                    match &original {
                        None => {
                            let order = match dao::select_all_remotes(&mut conn).await {
                                Ok(remotes) => remotes.len() + 1,
                                Err(e) => {
                                    return PreferencesMessage::SetOperationError(format!("{e}"));
                                }
                            };
                            match dao::insert_remote(
                                &mut conn,
                                NewRemote {
                                    base_url: url.to_string(),
                                    order: order as i32,
                                    user_id: user_id.clone(),
                                    passphrase: passphrase.clone(),
                                },
                            )
                            .await
                            {
                                Ok(_) => PreferencesMessage::SubmittedSource(
                                    original, url, user_id, passphrase,
                                ),
                                Err(error) => {
                                    PreferencesMessage::SetOperationError(format!("{error}"))
                                }
                            }
                        }
                        Some(existing) => {
                            match dao::update_remote(
                                &mut conn,
                                existing.id,
                                url.as_str(),
                                &user_id,
                                &passphrase,
                            )
                            .await
                            {
                                Ok(()) => PreferencesMessage::SubmittedSource(
                                    original, url, user_id, passphrase,
                                ),
                                Err(error) => {
                                    PreferencesMessage::SetOperationError(format!("{error}"))
                                }
                            }
                        }
                    }
                })
            }
            PreferencesMessage::SubmittedSource(original, url, user_id, passphrase) => {
                self.add_source_form = None;
                let output = match original {
                    None => PreferencesOutput::SourceAdded(url, user_id, passphrase),
                    Some(old) => PreferencesOutput::SourceEdited(
                        old.base_url.parse().unwrap(),
                        url,
                        user_id,
                        passphrase,
                    ),
                };
                task::message(PreferencesMessage::Out(output)).chain(task::message(
                    PreferencesMessage::Remotes(ProvidedStateMessage::Load),
                ))
            }
            PreferencesMessage::RequestDeleteSource(remote) => {
                self.pending_deletion = Some(remote);
                task::none()
            }
            PreferencesMessage::ConfirmDeleteSource => {
                if let Some(remote) = self.pending_deletion.take() {
                    task::message(PreferencesMessage::DeleteSource(remote.id))
                } else {
                    task::none()
                }
            }
            PreferencesMessage::CancelDeleteSource => {
                self.pending_deletion = None;
                task::none()
            }
            PreferencesMessage::DeleteSource(id) => {
                let am = Arc::clone(&self.application_module);
                task::future(async move {
                    let connection_pool = am.connection_pool().await;
                    match dao::delete_remote_by_id(&connection_pool, id).await {
                        Ok(_) => PreferencesMessage::DeletedSource(id),
                        Err(error) => PreferencesMessage::SetOperationError(format!("{error}")),
                    }
                })
            }
            PreferencesMessage::DeletedSource(id) => {
                let remote = self
                    .remotes_state
                    .state
                    .unwrap()
                    .iter()
                    .find(|a| a.id == id)
                    .unwrap();

                task::message(PreferencesMessage::Out(PreferencesOutput::SourceDeleted(
                    remote.base_url.parse().unwrap(),
                )))
                .chain(task::message(PreferencesMessage::Remotes(
                    ProvidedStateMessage::Load,
                )))
            }
            PreferencesMessage::SetOperationError(error) => {
                self.operation_error = Some(error);
                task::none()
            }
            PreferencesMessage::ClearOperationError => {
                self.operation_error = None;
                task::none()
            }
            PreferencesMessage::MoveSourceUp(remote) => {
                find_with_previous(self.remotes_state.state.unwrap().iter(), |current| {
                    current.id == remote.id
                })
                .map(|(prev, current)| {
                    task::message(PreferencesMessage::SwapOrderOfRemotes(
                        prev.clone(),
                        current.clone(),
                    ))
                })
                .unwrap_or_else(task::none)
            }
            PreferencesMessage::MoveSourceDown(remote) => {
                find_with_next(self.remotes_state.state.unwrap().iter(), |current| {
                    current.id == remote.id
                })
                .map(|(current, next)| {
                    task::message(PreferencesMessage::SwapOrderOfRemotes(
                        current.clone(),
                        next.clone(),
                    ))
                })
                .unwrap_or_else(task::none)
            }
            PreferencesMessage::SwapOrderOfRemotes(first, second) => {
                let am = Arc::clone(&self.application_module);
                task::future(async move {
                    let connection_pool = am.connection_pool().await;
                    match dao::swap_order_of_remotes(&connection_pool, &first, &second).await {
                        Ok(_) => PreferencesMessage::Remotes(ProvidedStateMessage::Load),
                        Err(error) => PreferencesMessage::SetOperationError(format!("{error}")),
                    }
                })
            }
            PreferencesMessage::RefreshStatuses => {
                if let LoadedState::Loaded(remotes) = &self.remotes_state.state {
                    let tasks: Vec<_> = remotes
                        .iter()
                        .map(|remote| {
                            task::message(PreferencesMessage::CheckSourceStatus(remote.clone()))
                        })
                        .collect();
                    Task::batch(tasks)
                } else {
                    task::none()
                }
            }
            PreferencesMessage::CheckSourceStatus(remote) => {
                self.source_statuses.insert(remote.id, LoadedState::Loading);
                task::future(async move {
                    let reachable = match remote.base_url.parse::<Url>() {
                        Ok(url) => {
                            let client =
                                FilesClient::new(url, remote.user_id, remote.passphrase, false)
                                    .unwrap();
                            client.status().await.is_ok()
                        }
                        Err(_) => false,
                    };
                    PreferencesMessage::SetSourceStatus(remote.id, reachable)
                })
            }
            PreferencesMessage::SetSourceStatus(id, reachable) => {
                self.source_statuses
                    .insert(id, LoadedState::Loaded(reachable));
                task::none()
            }
            PreferencesMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent component")
            }
            PreferencesMessage::Noop => task::none(),
        }
    }
}

fn make_user_entry(passphrase: String, roles: Vec<String>) -> UserEntry {
    let hashed: read_flow_core::settings::HashedPassword = passphrase.try_into().unwrap();
    if roles.is_empty() {
        UserEntry::Simple(hashed)
    } else {
        UserEntry::Extended {
            password: hashed,
            roles,
        }
    }
}

fn view_directory<'a>(
    path: &'a ExpandedPath,
    dir_settings: &'a DirectorySettings,
) -> widget::Row<'a, PreferencesMessage, Theme> {
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
        .on_press(PreferencesMessage::EditDirectory(path.clone().into()))
        .tooltip(fl!("settings-edit-directory"));

    let remove_button =
        widget::button::icon(icon::from_name("list-remove-symbolic").size(ICON_SIZE))
            .class(widget::button::ButtonClass::Destructive)
            .on_press(PreferencesMessage::RemoveDirectory(path.clone().into()))
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
