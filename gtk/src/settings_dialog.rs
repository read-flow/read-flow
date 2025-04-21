use gtk::prelude::*;
use relm4::component::AsyncComponent;
use relm4::component::AsyncComponentParts;
use relm4::component::AsyncComponentSender;
use relm4::gtk;
use relm4::RelmWidgetExt;
use std::sync::Arc;
use std::fs;
use std::path::PathBuf;
use std::io::Write;
use indexmap::IndexMap;
use std::collections::HashMap;

use archive_organizer::settings::{Settings, UiSettings};
use archive_organizer::scan::DirectorySettings;
use archive_organizer::ExpandedPath;

#[derive(Debug, Clone)]
pub struct SettingsDialog {
    settings: Arc<Settings>,
    // UI settings
    private_mode: bool,
    private_tags: Vec<String>,
    // Database settings
    db_path: String,
    // Scan settings
    dry_run: bool,
    auto_tags: HashMap<String, Vec<String>>,
    directories: HashMap<String, (String, bool, Vec<String>)>, // path -> (action, inherit, tags)
    // Widget references
    private_tags_entry: Option<gtk::Entry>,
    private_mode_switch: Option<gtk::Switch>,
    db_path_entry: Option<gtk::Entry>,
    dry_run_switch: Option<gtk::Switch>,
    auto_tags_text: Option<gtk::TextView>,
    directories_text: Option<gtk::TextView>,
    // Config path
    config_path: PathBuf,
}

#[derive(Debug)]
pub enum SettingsDialogInput {
    TogglePrivateMode(bool),
    UpdatePrivateTags(String),
    UpdateDbPath(String),
    ToggleDryRun(bool),
    UpdateAutoTags(String),
    UpdateDirectories(String),
    SaveSettings,
    Close,
}

#[derive(Debug)]
pub enum SettingsDialogOutput {
    SettingsSaved(Arc<Settings>),
    Closed,
}

impl SettingsDialog {
    fn get_config_path() -> PathBuf {
        if std::path::Path::new("Cargo.toml").exists() && std::path::Path::new("archive-organizer.toml").exists() {
            PathBuf::from("archive-organizer.toml")
                .canonicalize()
                .expect("should work for valid file")
        } else {
            let home = std::env::var("HOME").expect("HOME environment variable not set");
            PathBuf::from(format!("{home}/.config/archive-organizer/archive-organizer.toml"))
        }
    }

    fn save_settings(&self) -> Result<Arc<Settings>, std::io::Error> {
        // Read the current config file
        let config_path = self.config_path.clone();
        let content = fs::read_to_string(&config_path)?;

        // Parse the TOML content
        let mut doc = content.parse::<toml_edit::Document>()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        // Update the database settings
        if let Some(database) = doc.get_mut("database") {
            if let Some(table) = database.as_table_mut() {
                if let Some(url) = table.get_mut("url") {
                    *url = toml_edit::value(self.db_path.clone());
                } else {
                    table.insert("url", toml_edit::value(self.db_path.clone()));
                }
            }
        } else {
            // Create the database section if it doesn't exist
            let mut db_table = toml_edit::Table::new();
            db_table.insert("url", toml_edit::value(self.db_path.clone()));
            doc.insert("database", toml_edit::Item::Table(db_table));
        }

        // Update the scan settings
        if let Some(scan) = doc.get_mut("scan") {
            if let Some(table) = scan.as_table_mut() {
                // Update dry_run
                if let Some(dry_run) = table.get_mut("dry_run") {
                    *dry_run = toml_edit::value(self.dry_run);
                } else {
                    table.insert("dry_run", toml_edit::value(self.dry_run));
                }

                // Update auto_tags
                if let Some(auto_tags) = table.get_mut("auto_tags") {
                    if let Some(auto_tags_table) = auto_tags.as_table_mut() {
                        // Clear existing auto_tags
                        auto_tags_table.clear();

                        // Add new auto_tags
                        for (pattern, tags) in &self.auto_tags {
                            let tags_array = toml_edit::Array::from_iter(
                                tags.iter().map(|tag| toml_edit::Value::from(tag.clone()))
                            );
                            auto_tags_table.insert(pattern, toml_edit::value(toml_edit::Value::Array(tags_array)));
                        }
                    }
                } else {
                    // Create auto_tags table if it doesn't exist
                    let mut auto_tags_table = toml_edit::Table::new();
                    for (pattern, tags) in &self.auto_tags {
                        let tags_array = toml_edit::Array::from_iter(
                            tags.iter().map(|tag| toml_edit::Value::from(tag.clone()))
                        );
                        auto_tags_table.insert(pattern, toml_edit::value(toml_edit::Value::Array(tags_array)));
                    }
                    table.insert("auto_tags", toml_edit::Item::Table(auto_tags_table));
                }

                // Update directories
                if let Some(directories) = table.get_mut("directories") {
                    if let Some(directories_table) = directories.as_table_mut() {
                        // Clear existing directories
                        directories_table.clear();

                        // Add new directories
                        for (path, (action, inherit, tags)) in &self.directories {
                            let mut dir_table = toml_edit::Table::new();
                            dir_table.insert("action", toml_edit::value(action.clone()));
                            dir_table.insert("inherit", toml_edit::value(*inherit));

                            if action == "Scan" {
                                let tags_array = toml_edit::Array::from_iter(
                                    tags.iter().map(|tag| toml_edit::Value::from(tag.clone()))
                                );
                                dir_table.insert("tags", toml_edit::value(toml_edit::Value::Array(tags_array)));
                            }

                            directories_table.insert(path, toml_edit::Item::Table(dir_table));
                        }
                    }
                } else {
                    // Create directories table if it doesn't exist
                    let mut directories_table = toml_edit::Table::new();
                    for (path, (action, inherit, tags)) in &self.directories {
                        let mut dir_table = toml_edit::Table::new();
                        dir_table.insert("action", toml_edit::value(action.clone()));
                        dir_table.insert("inherit", toml_edit::value(*inherit));

                        if action == "Scan" {
                            let tags_array = toml_edit::Array::from_iter(
                                tags.iter().map(|tag| toml_edit::Value::from(tag.clone()))
                            );
                            dir_table.insert("tags", toml_edit::value(toml_edit::Value::Array(tags_array)));
                        }

                        directories_table.insert(path, toml_edit::Item::Table(dir_table));
                    }
                    table.insert("directories", toml_edit::Item::Table(directories_table));
                }
            }
        } else {
            // Create the scan section if it doesn't exist
            let mut scan_table = toml_edit::Table::new();
            scan_table.insert("dry_run", toml_edit::value(self.dry_run));

            // Add auto_tags
            let mut auto_tags_table = toml_edit::Table::new();
            for (pattern, tags) in &self.auto_tags {
                let tags_array = toml_edit::Array::from_iter(
                    tags.iter().map(|tag| toml_edit::Value::from(tag.clone()))
                );
                auto_tags_table.insert(pattern, toml_edit::value(toml_edit::Value::Array(tags_array)));
            }
            scan_table.insert("auto_tags", toml_edit::Item::Table(auto_tags_table));

            // Add directories
            let mut directories_table = toml_edit::Table::new();
            for (path, (action, inherit, tags)) in &self.directories {
                let mut dir_table = toml_edit::Table::new();
                dir_table.insert("action", toml_edit::value(action.clone()));
                dir_table.insert("inherit", toml_edit::value(*inherit));

                if action == "Scan" {
                    let tags_array = toml_edit::Array::from_iter(
                        tags.iter().map(|tag| toml_edit::Value::from(tag.clone()))
                    );
                    dir_table.insert("tags", toml_edit::value(toml_edit::Value::Array(tags_array)));
                }

                directories_table.insert(path, toml_edit::Item::Table(dir_table));
            }
            scan_table.insert("directories", toml_edit::Item::Table(directories_table));

            doc.insert("scan", toml_edit::Item::Table(scan_table));
        }

        // Update the UI settings
        if let Some(ui) = doc.get_mut("ui") {
            if let Some(table) = ui.as_table_mut() {
                // Update private_mode
                if let Some(private_mode) = table.get_mut("private_mode") {
                    *private_mode = toml_edit::value(self.private_mode);
                } else {
                    table.insert("private_mode", toml_edit::value(self.private_mode));
                }

                // Update private_tags
                let tags_array = toml_edit::Array::from_iter(
                    self.private_tags.iter().map(|tag| toml_edit::Value::from(tag.clone()))
                );
                if let Some(private_tags) = table.get_mut("private_tags") {
                    *private_tags = toml_edit::value(toml_edit::Value::Array(tags_array));
                } else {
                    table.insert("private_tags", toml_edit::value(toml_edit::Value::Array(tags_array)));
                }
            }
        } else {
            // Create the ui section if it doesn't exist
            let mut ui_table = toml_edit::Table::new();
            ui_table.insert("private_mode", toml_edit::value(self.private_mode));

            let tags_array = toml_edit::Array::from_iter(
                self.private_tags.iter().map(|tag| toml_edit::Value::from(tag.clone()))
            );
            ui_table.insert("private_tags", toml_edit::value(toml_edit::Value::Array(tags_array)));

            doc.insert("ui", toml_edit::Item::Table(ui_table));
        }

        // Write the updated TOML back to the file
        let mut file = fs::File::create(&config_path)?;
        file.write_all(doc.to_string().as_bytes())?;

        // Create a new Settings object with the updated values
        // We'll clone the original settings and update what we can
        let mut new_settings = (*self.settings).clone();

        // Update the UI settings
        new_settings.ui = UiSettings::new(self.private_mode, self.private_tags.clone());

        // Update scan settings
        new_settings.scan.dry_run = self.dry_run;

        // We can't update auto_tags and directories directly because they use IndexMap and ExpandedPath
        // which we can't easily recreate here. The changes will be saved to the config file though.

        Ok(Arc::new(new_settings))
    }
}

#[relm4::component(pub, async)]
impl AsyncComponent for SettingsDialog {
    type Init = Arc<Settings>;
    type Input = SettingsDialogInput;
    type Output = SettingsDialogOutput;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Dialog {
            set_title: Some("Settings"),
            set_modal: true,
            set_width_request: 600,
            set_height_request: 500,
            set_hide_on_close: true,
            present: (),

            #[wrap(Some)]
            set_child = &gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 20,
                set_margin_all: 20,

                // Title and Config Path
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 5,
                    set_margin_bottom: 10,

                    gtk::Label {
                        set_markup: "<span size='large' weight='bold'>Settings</span>",
                        set_halign: gtk::Align::Start,
                    },

                    gtk::Label {
                        set_markup: &format!("<span size='small'>Configuration file: {}</span>", model.config_path.display()),
                        set_halign: gtk::Align::Start,
                        set_margin_bottom: 10,
                    },
                },

                // Settings Sections in a ScrolledWindow
                gtk::ScrolledWindow {
                    set_vexpand: true,
                    set_hscrollbar_policy: gtk::PolicyType::Never,

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 20,
                        set_margin_all: 10,

                        // Database Settings Section
                        gtk::Frame {
                            set_label: Some("Database Settings"),
                            set_label_align: 0.5,
                            set_margin_top: 10,

                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_spacing: 10,
                                set_margin_all: 10,

                                gtk::Box {
                                    set_orientation: gtk::Orientation::Horizontal,
                                    set_spacing: 10,
                                    set_margin_bottom: 10,

                                    gtk::Label {
                                        set_text: "Database Path:",
                                        set_halign: gtk::Align::Start,
                                        set_hexpand: false,
                                        set_width_request: 120,
                                    },

                                    #[name(db_path_entry)]
                                    gtk::Entry {
                                        set_text: &model.db_path,
                                        set_hexpand: true,
                                        connect_changed[sender] => move |entry| {
                                            sender.input(SettingsDialogInput::UpdateDbPath(entry.text().to_string()));
                                        }
                                    }
                                },

                                gtk::Label {
                                    set_text: "The database path specifies the location of the SQLite database file.",
                                    set_wrap: true,
                                    set_margin_top: 5,
                                    set_halign: gtk::Align::Start,
                                },
                            }
                        },

                        // Scan Settings Section
                        gtk::Frame {
                            set_label: Some("Scan Settings"),
                            set_label_align: 0.5,
                            set_margin_top: 10,

                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_spacing: 10,
                                set_margin_all: 10,

                                gtk::Box {
                                    set_orientation: gtk::Orientation::Horizontal,
                                    set_spacing: 10,
                                    set_margin_bottom: 10,

                                    gtk::Label {
                                        set_text: "Dry Run:",
                                        set_halign: gtk::Align::Start,
                                        set_hexpand: true,
                                    },

                                    #[name(dry_run_switch)]
                                    gtk::Switch {
                                        set_active: model.dry_run,
                                        set_halign: gtk::Align::End,
                                        connect_state_set[sender] => move |_, state| {
                                            sender.input(SettingsDialogInput::ToggleDryRun(state));
                                            false.into()
                                        }
                                    }
                                },

                                gtk::Label {
                                    set_text: "When Dry Run is enabled, scanning will not make changes to the database.",
                                    set_wrap: true,
                                    set_margin_top: 5,
                                    set_margin_bottom: 15,
                                    set_halign: gtk::Align::Start,
                                },

                                gtk::Label {
                                    set_markup: "<span weight='bold'>Auto Tags</span>",
                                    set_halign: gtk::Align::Start,
                                    set_margin_bottom: 5,
                                },

                                gtk::Label {
                                    set_text: "Define patterns to automatically apply tags to files. Format: one pattern=tag1,tag2 per line",
                                    set_wrap: true,
                                    set_halign: gtk::Align::Start,
                                    set_margin_bottom: 5,
                                },

                                gtk::ScrolledWindow {
                                    set_min_content_height: 100,
                                    set_vexpand: false,

                                    #[name(auto_tags_text)]
                                    gtk::TextView {
                                        set_wrap_mode: gtk::WrapMode::Word,
                                        set_monospace: true,

                                    }
                                },

                                gtk::Label {
                                    set_markup: "<span weight='bold'>Directory Settings</span>",
                                    set_halign: gtk::Align::Start,
                                    set_margin_top: 15,
                                    set_margin_bottom: 5,
                                },

                                gtk::Label {
                                    set_text: "Define scan options for specific folders. Format: path=action,inherit,tag1,tag2 (action is 'Scan' or 'Ignore')",
                                    set_wrap: true,
                                    set_halign: gtk::Align::Start,
                                    set_margin_bottom: 5,
                                },

                                gtk::ScrolledWindow {
                                    set_min_content_height: 100,
                                    set_vexpand: false,

                                    #[name(directories_text)]
                                    gtk::TextView {
                                        set_wrap_mode: gtk::WrapMode::Word,
                                        set_monospace: true,

                                    }
                                },
                            }
                        },

                        // UI Settings Section
                        gtk::Frame {
                            set_label: Some("UI Settings"),
                            set_label_align: 0.5,
                            set_margin_top: 10,

                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_spacing: 10,
                                set_margin_all: 10,

                                gtk::Box {
                                    set_orientation: gtk::Orientation::Horizontal,
                                    set_spacing: 10,
                                    set_margin_bottom: 10,

                                    gtk::Label {
                                        set_text: "Private Mode:",
                                        set_halign: gtk::Align::Start,
                                        set_hexpand: true,
                                    },

                                    #[name(private_mode_switch)]
                                    gtk::Switch {
                                        set_active: model.private_mode,
                                        set_halign: gtk::Align::End,
                                        connect_state_set[sender] => move |_, state| {
                                            sender.input(SettingsDialogInput::TogglePrivateMode(state));
                                            false.into()
                                        }
                                    }
                                },

                                gtk::Box {
                                    set_orientation: gtk::Orientation::Vertical,
                                    set_spacing: 10,
                                    set_margin_bottom: 10,

                                    gtk::Label {
                                        set_text: "Private Tags (comma-separated):",
                                        set_halign: gtk::Align::Start,
                                    },

                                    #[name(private_tags_entry)]
                                    gtk::Entry {
                                        set_text: &model.private_tags.join(", "),
                                        set_hexpand: true,
                                        connect_changed[sender] => move |entry| {
                                            sender.input(SettingsDialogInput::UpdatePrivateTags(entry.text().to_string()));
                                        }
                                    }
                                },

                                gtk::Label {
                                    set_text: "Private tags and files with private tags will be hidden from the file list unless Private Mode is enabled.",
                                    set_wrap: true,
                                    set_margin_top: 5,
                                    set_halign: gtk::Align::Start,
                                },
                            }
                        },
                    }
                },

                // Buttons
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 10,
                    set_halign: gtk::Align::End,
                    set_margin_top: 10,

                    gtk::Button {
                        set_label: "Cancel",
                        connect_clicked[sender] => move |_| {
                            sender.input(SettingsDialogInput::Close);
                        }
                    },

                    gtk::Button {
                        set_label: "Save",
                        add_css_class: "suggested-action",
                        connect_clicked[sender] => move |_| {
                            sender.input(SettingsDialogInput::SaveSettings);
                        }
                    }
                }
            }
        }
    }

    async fn init(
        settings: Self::Init,
        root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        // Extract current settings
        let private_mode = settings.ui.hidden_tags().is_empty();
        let private_tags = settings.ui.hidden_tags().to_vec();

        // Get database path from settings
        // Since we can't access the private url field directly, we'll use a default path
        let db_url = "$HOME/.local/share/archive-organizer/database.db".to_string();

        let dry_run = settings.scan.dry_run;

        // Convert auto_tags from IndexMap to HashMap
        let mut auto_tags = HashMap::new();
        for (pattern, tags) in &settings.scan.auto_tags {
            auto_tags.insert(pattern.to_string(), tags.clone());
        }

        // Convert directories from IndexMap<ExpandedPath, DirectorySettings> to HashMap<String, (String, bool, Vec<String>)>
        let mut directories = HashMap::new();
        for (path, dir_settings) in &settings.scan.directories {
            let path_str = path.to_string_lossy().to_string();
            match dir_settings {
                DirectorySettings::Ignore { inherit } => {
                    directories.insert(path_str, ("Ignore".to_string(), *inherit, Vec::new()));
                }
                DirectorySettings::Scan { tags, inherit } => {
                    directories.insert(path_str, ("Scan".to_string(), *inherit, tags.clone()));
                }
            }
        }
        let config_path = Self::get_config_path();

        let model = SettingsDialog {
            settings,
            private_mode,
            private_tags,
            db_path: db_url,
            dry_run,
            auto_tags,
            directories,
            private_tags_entry: None,
            private_mode_switch: None,
            db_path_entry: None,
            dry_run_switch: None,
            auto_tags_text: None,
            directories_text: None,
            config_path,
        };

        let widgets = view_output!();

        // Store references to widgets
        let mut model = model;
        model.private_tags_entry = Some(widgets.private_tags_entry.clone());
        model.private_mode_switch = Some(widgets.private_mode_switch.clone());
        model.db_path_entry = Some(widgets.db_path_entry.clone());
        model.dry_run_switch = Some(widgets.dry_run_switch.clone());
        model.auto_tags_text = Some(widgets.auto_tags_text.clone());
        model.directories_text = Some(widgets.directories_text.clone());

        // Set initial text for auto_tags and directories
        if let Some(text_view) = &model.auto_tags_text {
            let buffer = text_view.buffer();
            let mut auto_tags_text = String::new();
            for (pattern, tags) in &model.auto_tags {
                auto_tags_text.push_str(&format!("{} = {}\n", pattern, tags.join(", ")));
            }
            buffer.set_text(&auto_tags_text);

            // Connect to the buffer's changed signal
            let sender_clone = sender.input_sender().clone();
            buffer.connect_changed(move |buffer| {
                let start = buffer.start_iter();
                let end = buffer.end_iter();
                let text = buffer.text(&start, &end, false).to_string();
                sender_clone.send(SettingsDialogInput::UpdateAutoTags(text)).unwrap();
            });
        }

        if let Some(text_view) = &model.directories_text {
            let buffer = text_view.buffer();
            let mut directories_text = String::new();
            for (path, (action, inherit, tags)) in &model.directories {
                if action == "Scan" {
                    directories_text.push_str(&format!("{} = {}, {}, {}\n", path, action, inherit, tags.join(", ")));
                } else {
                    directories_text.push_str(&format!("{} = {}, {}\n", path, action, inherit));
                }
            }
            buffer.set_text(&directories_text);

            // Connect to the buffer's changed signal
            let sender_clone = sender.input_sender().clone();
            buffer.connect_changed(move |buffer| {
                let start = buffer.start_iter();
                let end = buffer.end_iter();
                let text = buffer.text(&start, &end, false).to_string();
                sender_clone.send(SettingsDialogInput::UpdateDirectories(text)).unwrap();
            });
        }

        AsyncComponentParts { model, widgets }
    }

    async fn update(
        &mut self,
        msg: Self::Input,
        sender: AsyncComponentSender<Self>,
        root: &Self::Root,
    ) {
        match msg {
            SettingsDialogInput::TogglePrivateMode(state) => {
                self.private_mode = state;
            }
            SettingsDialogInput::UpdatePrivateTags(tags_str) => {
                // Parse comma-separated tags
                self.private_tags = tags_str
                    .split(',')
                    .map(|tag| tag.trim().to_string())
                    .filter(|tag| !tag.is_empty())
                    .collect();
            }
            SettingsDialogInput::UpdateDbPath(path) => {
                self.db_path = path;
            }
            SettingsDialogInput::ToggleDryRun(state) => {
                self.dry_run = state;
            }
            SettingsDialogInput::UpdateAutoTags(text) => {
                // Parse auto_tags from text
                self.auto_tags.clear();
                for line in text.lines() {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }

                    if let Some((pattern, tags_str)) = line.split_once('=') {
                        let pattern = pattern.trim().to_string();
                        let tags: Vec<String> = tags_str
                            .split(',')
                            .map(|tag| tag.trim().to_string())
                            .filter(|tag| !tag.is_empty())
                            .collect();

                        if !pattern.is_empty() && !tags.is_empty() {
                            self.auto_tags.insert(pattern, tags);
                        }
                    }
                }
            }
            SettingsDialogInput::UpdateDirectories(text) => {
                // Parse directories from text
                self.directories.clear();
                for line in text.lines() {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }

                    if let Some((path, settings_str)) = line.split_once('=') {
                        let path = path.trim().to_string();
                        let parts: Vec<&str> = settings_str.split(',').map(|s| s.trim()).collect();

                        if parts.len() >= 2 && !path.is_empty() {
                            let action = parts[0].to_string();
                            let inherit = parts[1].parse::<bool>().unwrap_or(false);

                            if action == "Scan" && parts.len() > 2 {
                                let tags: Vec<String> = parts[2..]
                                    .iter()
                                    .map(|tag| tag.trim().to_string())
                                    .filter(|tag| !tag.is_empty())
                                    .collect();

                                self.directories.insert(path, (action, inherit, tags));
                            } else if action == "Ignore" {
                                self.directories.insert(path, (action, inherit, Vec::new()));
                            }
                        }
                    }
                }
            }
            SettingsDialogInput::SaveSettings => {
                match self.save_settings() {
                    Ok(new_settings) => {
                        sender.output(SettingsDialogOutput::SettingsSaved(new_settings)).unwrap();
                        root.close();
                    }
                    Err(e) => {
                        tracing::error!("Failed to save settings: {}", e);
                        // TODO: Show error dialog
                    }
                }
            }
            SettingsDialogInput::Close => {
                root.close();
                sender.output(SettingsDialogOutput::Closed).unwrap();
            }
        }
    }
}
