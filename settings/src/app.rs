use std::path::PathBuf;

use iced::Element;
use iced::Length;
use iced::Task;
use iced::widget::button;
use iced::widget::column;
use iced::widget::container;
use iced::widget::row;
use iced::widget::rule;
use iced::widget::scrollable;
use iced::widget::text;
use read_flow_core::ExpandedPath;
use read_flow_core::scan::DocumentType;
use read_flow_core::settings::Settings;

use crate::pages::client;
use crate::pages::database;
use crate::pages::online_library;
use crate::pages::online_library::CatalogForm;
use crate::pages::online_library::CatalogFormMessage;
use crate::pages::scan;
use crate::pages::server;
use crate::pages::ui;
use crate::save::SaveState;
use crate::section::Section;
use crate::widgets::auto_tags::AutoTagForm;
use crate::widgets::auto_tags::AutoTagFormMessage;
use crate::widgets::dir_editor::DirForm;
use crate::widgets::dir_editor::DirFormMessage;
use crate::widgets::user_editor::UserForm;
use crate::widgets::user_editor::UserFormMessage;

pub struct App {
    pub config_path: PathBuf,
    pub original_settings: Settings,
    pub settings: Settings,
    pub section: Section,
    pub save_state: SaveState,

    pub dir_form: Option<DirForm>,
    pub user_form: Option<UserForm>,
    pub catalog_form: Option<CatalogForm>,
    pub auto_tag_form: Option<AutoTagForm>,

    pub private_tag_input: String,
    pub concurrency_input: String,
}

impl App {
    pub fn new(config_path: PathBuf, settings: Settings) -> (Self, Task<Message>) {
        let concurrency_input = settings.scan.concurrency.to_string();
        let app = App {
            config_path,
            original_settings: settings.clone(),
            settings,
            section: Section::default(),
            save_state: SaveState::default(),
            dir_form: None,
            user_form: None,
            catalog_form: None,
            auto_tag_form: None,
            private_tag_input: String::new(),
            concurrency_input,
        };
        (app, Task::none())
    }

    pub fn is_modified(&self) -> bool {
        self.settings != self.original_settings
    }

    pub fn title(&self) -> String {
        if self.is_modified() {
            "read-flow Settings *".into()
        } else {
            "read-flow Settings".into()
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::SectionChanged(section) => {
                self.section = section;
                self.dir_form = None;
                self.user_form = None;
                self.catalog_form = None;
                self.auto_tag_form = None;
                Task::none()
            }

            // Database
            Message::PickDatabaseFile => {
                let start = self.settings.database.url().get_directory();
                Task::perform(
                    async move {
                        let mut dialog = rfd::AsyncFileDialog::new();
                        if let Some(dir) = start {
                            dialog = dialog.set_directory(dir);
                        }
                        dialog.pick_file().await.map(|h| h.path().to_path_buf())
                    },
                    Message::DatabaseFilePicked,
                )
            }
            Message::DatabaseFilePicked(Some(path)) => {
                if let Ok(expanded) = path.try_into() {
                    self.settings.database.set_url(expanded);
                }
                Task::none()
            }
            Message::DatabaseFilePicked(None) => Task::none(),

            // Client
            Message::PickClientFolder => {
                let start = self.settings.client.download_folder.get_directory();
                Task::perform(
                    async move {
                        let mut dialog = rfd::AsyncFileDialog::new();
                        if let Some(dir) = start {
                            dialog = dialog.set_directory(dir);
                        }
                        dialog.pick_folder().await.map(|h| h.path().to_path_buf())
                    },
                    Message::ClientFolderPicked,
                )
            }
            Message::ClientFolderPicked(Some(path)) => {
                if let Ok(expanded) = path.try_into() {
                    self.settings.client.download_folder = expanded;
                }
                Task::none()
            }
            Message::ClientFolderPicked(None) => Task::none(),

            // Server folder
            Message::PickServerFolder => {
                let start = self.settings.server.download_folder.get_directory();
                Task::perform(
                    async move {
                        let mut dialog = rfd::AsyncFileDialog::new();
                        if let Some(dir) = start {
                            dialog = dialog.set_directory(dir);
                        }
                        dialog.pick_folder().await.map(|h| h.path().to_path_buf())
                    },
                    Message::ServerFolderPicked,
                )
            }
            Message::ServerFolderPicked(Some(path)) => {
                if let Ok(expanded) = path.try_into() {
                    self.settings.server.download_folder = expanded;
                }
                Task::none()
            }
            Message::ServerFolderPicked(None) => Task::none(),

            // Scan toggles
            Message::ToggleDryRun(v) => {
                self.settings.scan.dry_run = v;
                Task::none()
            }
            Message::ToggleExtension(dt, enabled) => {
                if enabled {
                    if !self.settings.scan.extensions.contains(&dt) {
                        self.settings.scan.extensions.push(dt);
                    }
                } else {
                    self.settings.scan.extensions.retain(|e| e != &dt);
                }
                Task::none()
            }
            Message::ToggleAllExtensions(enabled) => {
                if enabled {
                    let all: Vec<DocumentType> = DocumentType::all().to_vec();
                    self.settings.scan.extensions = all;
                } else {
                    self.settings.scan.extensions.clear();
                }
                Task::none()
            }
            Message::ConcurrencyChanged(s) => {
                self.concurrency_input = s.clone();
                if let Ok(n) = s.parse::<usize>() {
                    self.settings.scan.concurrency = n;
                }
                Task::none()
            }

            // Directory list
            Message::DirAddStart => {
                self.dir_form = Some(DirForm::new_empty());
                Task::none()
            }
            Message::DirEditStart(key) => {
                if let Some(settings) = self.settings.scan.directories.get(&key) {
                    self.dir_form = Some(DirForm::from_entry(key, settings));
                }
                Task::none()
            }
            Message::DirRemove(key) => {
                self.settings.scan.directories.remove(&key);
                Task::none()
            }
            Message::DirBrowse => {
                let current = self
                    .dir_form
                    .as_ref()
                    .and_then(|f| f.path.parse::<ExpandedPath>().ok())
                    .and_then(|e| e.get_directory());
                Task::perform(
                    async move {
                        let mut dialog = rfd::AsyncFileDialog::new();
                        if let Some(dir) = current {
                            dialog = dialog.set_directory(dir);
                        }
                        dialog.pick_folder().await.map(|h| h.path().to_path_buf())
                    },
                    Message::DirBrowsePicked,
                )
            }
            Message::DirBrowsePicked(Some(path)) => {
                if let Some(form) = &mut self.dir_form {
                    form.path = path.display().to_string();
                }
                Task::none()
            }
            Message::DirBrowsePicked(None) => Task::none(),
            Message::DirForm(msg) => {
                if let Some(form) = &mut self.dir_form {
                    match msg {
                        DirFormMessage::PathChanged(s) => form.path = s,
                        DirFormMessage::ActionChanged(a) => form.action = a,
                        DirFormMessage::TagInput(s) => form.tag_input = s,
                        DirFormMessage::AddTag => {
                            let tag = form.tag_input.trim().to_string();
                            if !tag.is_empty() && !form.tags.contains(&tag) {
                                form.tags.push(tag);
                            }
                            form.tag_input.clear();
                        }
                        DirFormMessage::RemoveTag(t) => form.tags.retain(|x| x != &t),
                        DirFormMessage::InheritToggled(b) => form.inherit = b,
                    }
                }
                Task::none()
            }
            Message::DirSave => {
                if let Some(form) = self.dir_form.take() {
                    if let Ok(key) = form.path.parse::<ExpandedPath>() {
                        if let Some(old_key) = &form.original_key {
                            self.settings.scan.directories.remove(old_key);
                        }
                        self.settings
                            .scan
                            .directories
                            .insert(key, form.to_directory_settings());
                    }
                }
                Task::none()
            }
            Message::DirCancel => {
                self.dir_form = None;
                Task::none()
            }

            // Auto-tags
            Message::AutoTagAddStart => {
                self.auto_tag_form = Some(AutoTagForm::new_empty());
                Task::none()
            }
            Message::AutoTagEditStart(key) => {
                if let Some(tags) = self.settings.scan.auto_tags.get(&key) {
                    self.auto_tag_form = Some(AutoTagForm::from_entry(key, tags.clone()));
                }
                Task::none()
            }
            Message::AutoTagRemove(key) => {
                self.settings.scan.auto_tags.remove(&key);
                Task::none()
            }
            Message::AutoTagForm(msg) => {
                if let Some(form) = &mut self.auto_tag_form {
                    match msg {
                        AutoTagFormMessage::PatternChanged(s) => form.pattern = s,
                        AutoTagFormMessage::TagInput(s) => form.tag_input = s,
                        AutoTagFormMessage::AddTag => {
                            let tag = form.tag_input.trim().to_string();
                            if !tag.is_empty() && !form.tags.contains(&tag) {
                                form.tags.push(tag);
                            }
                            form.tag_input.clear();
                        }
                        AutoTagFormMessage::RemoveTag(t) => form.tags.retain(|x| x != &t),
                    }
                }
                Task::none()
            }
            Message::AutoTagSave => {
                if let Some(form) = self.auto_tag_form.take() {
                    if !form.pattern.is_empty() {
                        if let Some(old_key) = &form.original_key {
                            self.settings.scan.auto_tags.remove(old_key);
                        }
                        self.settings
                            .scan
                            .auto_tags
                            .insert(form.pattern.clone(), form.tags.clone());
                    }
                }
                Task::none()
            }
            Message::AutoTagCancel => {
                self.auto_tag_form = None;
                Task::none()
            }

            // Server users
            Message::UserAddStart => {
                self.user_form = Some(UserForm::new_empty());
                Task::none()
            }
            Message::UserEditStart(id) => {
                if let Some(entry) = self.settings.server.authorized_users.get(&id) {
                    self.user_form = Some(UserForm::from_entry(id, entry));
                }
                Task::none()
            }
            Message::UserDelete(id) => {
                self.settings.server.authorized_users.shift_remove(&id);
                Task::none()
            }
            Message::UserForm(msg) => {
                if let Some(form) = &mut self.user_form {
                    match msg {
                        UserFormMessage::UserIdChanged(s) => form.user_id = s,
                        UserFormMessage::PasswordChanged(s) => form.new_password = s,
                        UserFormMessage::OwnerRoleToggled(b) => form.owner_role = b,
                    }
                }
                Task::none()
            }
            Message::UserSave => {
                if let Some(form) = self.user_form.take() {
                    let existing = form
                        .original_id
                        .as_deref()
                        .and_then(|id| self.settings.server.authorized_users.get(id));
                    match form.to_user_entry(existing) {
                        Ok(entry) => {
                            let id = form
                                .original_id
                                .clone()
                                .unwrap_or_else(|| form.user_id.clone());
                            if let Some(old_id) = &form.original_id {
                                self.settings.server.authorized_users.shift_remove(old_id);
                            }
                            if !id.is_empty() {
                                self.settings.server.authorized_users.insert(id, entry);
                            }
                        }
                        Err(e) => {
                            self.save_state = SaveState::Error(e);
                        }
                    }
                }
                Task::none()
            }
            Message::UserCancel => {
                self.user_form = None;
                Task::none()
            }

            // UI settings
            Message::TogglePrivateMode(v) => {
                self.settings.ui.set_private_mode(v);
                Task::none()
            }
            Message::PrivateTagInput(s) => {
                self.private_tag_input = s;
                Task::none()
            }
            Message::AddPrivateTag => {
                let tag = self.private_tag_input.trim().to_string();
                if !tag.is_empty() {
                    let mut tags = self.settings.ui.private_tags().to_vec();
                    if !tags.contains(&tag) {
                        tags.push(tag);
                    }
                    self.settings.ui.set_private_tags(tags);
                }
                self.private_tag_input.clear();
                Task::none()
            }
            Message::RemovePrivateTag(tag) => {
                let tags: Vec<String> = self
                    .settings
                    .ui
                    .private_tags()
                    .iter()
                    .filter(|t| *t != &tag)
                    .cloned()
                    .collect();
                self.settings.ui.set_private_tags(tags);
                Task::none()
            }

            // Online library
            Message::CatalogAddStart => {
                self.catalog_form = Some(CatalogForm::new_empty());
                Task::none()
            }
            Message::CatalogEditStart(i) => {
                if let Some(cat) = self.settings.online_library.catalogs.get(i) {
                    self.catalog_form = Some(CatalogForm::from_catalog(i, cat));
                }
                Task::none()
            }
            Message::CatalogRemove(i) => {
                if i < self.settings.online_library.catalogs.len() {
                    self.settings.online_library.catalogs.remove(i);
                }
                Task::none()
            }
            Message::CatalogToggleEnabled(i) => {
                if let Some(cat) = self.settings.online_library.catalogs.get_mut(i) {
                    cat.enabled = !cat.enabled;
                }
                Task::none()
            }
            Message::CatalogForm(msg) => {
                if let Some(form) = &mut self.catalog_form {
                    match msg {
                        CatalogFormMessage::NameChanged(s) => form.name = s,
                        CatalogFormMessage::SearchUrlChanged(s) => form.search_url = s,
                        CatalogFormMessage::EnabledToggled(b) => form.enabled = b,
                    }
                }
                Task::none()
            }
            Message::CatalogSave => {
                if let Some(form) = self.catalog_form.take() {
                    let catalog = form.to_catalog();
                    match form.original_index {
                        Some(i) if i < self.settings.online_library.catalogs.len() => {
                            self.settings.online_library.catalogs[i] = catalog;
                        }
                        _ => {
                            self.settings.online_library.catalogs.push(catalog);
                        }
                    }
                }
                Task::none()
            }
            Message::CatalogCancel => {
                self.catalog_form = None;
                Task::none()
            }

            // Save
            Message::Save => {
                self.save_state = SaveState::Saving;
                let settings = self.settings.clone();
                let path = self.config_path.clone();
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            settings.save(&path).map_err(|e| e.to_string())
                        })
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    Message::SaveResult,
                )
            }
            Message::SaveResult(Ok(())) => {
                self.save_state = SaveState::Saved;
                self.original_settings = self.settings.clone();
                Task::none()
            }
            Message::SaveResult(Err(e)) => {
                self.save_state = SaveState::Error(format!("Error: {e}"));
                Task::none()
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let content: Element<'_, Message> = match &self.section {
            Section::Overview => self.view_overview(),
            Section::Database => {
                self.view_subsection(database::view_database(&self.settings.database))
            }
            Section::Client => self.view_subsection(client::view_client(&self.settings.client)),
            Section::Scan => self.view_subsection(scan::view_scan(
                &self.settings.scan,
                self.dir_form.as_ref(),
                self.auto_tag_form.as_ref(),
                &self.concurrency_input,
            )),
            Section::Server => self.view_subsection(server::view_server(
                &self.settings.server,
                self.user_form.as_ref(),
            )),
            Section::Ui => {
                self.view_subsection(ui::view_ui(&self.settings.ui, &self.private_tag_input))
            }
            Section::OnlineLibrary => self.view_subsection(online_library::view_online_library(
                &self.settings.online_library,
                self.catalog_form.as_ref(),
            )),
        };

        let status_text = self.save_state.status_text();
        let save_btn = {
            let btn = button(text("Save")).on_press(Message::Save);
            if self.is_modified() {
                btn.style(button::primary)
            } else {
                btn.style(button::secondary)
            }
        };

        let bottom_bar = container(
            row![save_btn, text(status_text).width(Length::Fill)]
                .spacing(10)
                .align_y(iced::Alignment::Center)
                .padding(8),
        )
        .width(Length::Fill);

        column![
            scrollable(content).height(Length::Fill),
            rule::horizontal(1),
            bottom_bar,
        ]
        .height(Length::Fill)
        .into()
    }

    fn view_overview(&self) -> Element<'_, Message> {
        let cards: Vec<Element<'_, Message>> = Section::all()
            .iter()
            .map(|s| {
                button(
                    row![
                        column![text(s.label()).size(15), text(s.description()).size(12),]
                            .width(Length::Fill),
                        text("\u{203a}").size(22),
                    ]
                    .align_y(iced::Alignment::Center)
                    .padding([4, 0]),
                )
                .style(|theme: &iced::Theme, status| {
                    use iced::Border;
                    use iced::border::Radius;
                    let palette = theme.extended_palette();
                    let bg = match status {
                        button::Status::Hovered | button::Status::Pressed => {
                            palette.background.strong.color
                        }
                        _ => palette.background.weak.color,
                    };
                    button::Style {
                        background: Some(bg.into()),
                        text_color: palette.background.weak.text,
                        border: Border {
                            radius: Radius::from(8.0),
                            ..Default::default()
                        },
                        ..Default::default()
                    }
                })
                .width(Length::Fill)
                .on_press(Message::SectionChanged(s.clone()))
                .into()
            })
            .collect();

        column![
            text("Settings").size(24).width(Length::Fill),
            column(cards).spacing(8),
        ]
        .spacing(16)
        .padding(20)
        .into()
    }

    fn view_subsection<'a>(&'a self, content: Element<'a, Message>) -> Element<'a, Message> {
        let back = button(text("\u{2039} Back"))
            .style(button::text)
            .on_press(Message::SectionChanged(Section::Overview));

        column![back, content].into()
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    SectionChanged(Section),

    // Database
    PickDatabaseFile,
    DatabaseFilePicked(Option<PathBuf>),

    // Client
    PickClientFolder,
    ClientFolderPicked(Option<PathBuf>),

    // Server folder
    PickServerFolder,
    ServerFolderPicked(Option<PathBuf>),

    // Scan
    ToggleDryRun(bool),
    ToggleExtension(DocumentType, bool),
    ToggleAllExtensions(bool),
    ConcurrencyChanged(String),

    // Directory list
    DirAddStart,
    DirEditStart(ExpandedPath),
    DirRemove(ExpandedPath),
    DirBrowse,
    DirBrowsePicked(Option<PathBuf>),
    DirForm(DirFormMessage),
    DirSave,
    DirCancel,

    // Auto-tags
    AutoTagAddStart,
    AutoTagEditStart(String),
    AutoTagRemove(String),
    AutoTagForm(AutoTagFormMessage),
    AutoTagSave,
    AutoTagCancel,

    // Server users
    UserAddStart,
    UserEditStart(String),
    UserDelete(String),
    UserForm(UserFormMessage),
    UserSave,
    UserCancel,

    // UI
    TogglePrivateMode(bool),
    PrivateTagInput(String),
    AddPrivateTag,
    RemovePrivateTag(String),

    // Online library
    CatalogAddStart,
    CatalogEditStart(usize),
    CatalogRemove(usize),
    CatalogToggleEnabled(usize),
    CatalogForm(CatalogFormMessage),
    CatalogSave,
    CatalogCancel,

    // Save
    Save,
    SaveResult(Result<(), String>),
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use read_flow_core::settings::Settings;

    use super::*;

    fn make_app() -> App {
        let (app, _) = App::new(PathBuf::from("test.toml"), Settings::default());
        app
    }

    #[test]
    fn is_modified_false_on_creation() {
        let app = make_app();
        assert!(!app.is_modified());
    }

    #[test]
    fn is_modified_true_after_dry_run_toggle() {
        let mut app = make_app();
        let _ = app.update(Message::ToggleDryRun(true));
        assert!(app.is_modified());
    }

    #[test]
    fn is_modified_false_after_revert() {
        let mut app = make_app();
        let _ = app.update(Message::ToggleDryRun(true));
        let _ = app.update(Message::ToggleDryRun(false));
        assert!(!app.is_modified());
    }
}
