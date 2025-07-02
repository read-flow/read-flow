use archive_organizer::ApplicationModule;
use archive_organizer::client::FilesClient;
use archive_organizer::db::datasource::DbClient;
use gtk::prelude::*;
use indexmap::IndexMap;
use relm4::component::AsyncComponent;
use relm4::component::AsyncComponentController;
use relm4::component::AsyncComponentParts;
use relm4::component::AsyncComponentSender;
use relm4::component::AsyncController;
use relm4::gtk;
use relm4::loading_widgets::LoadingWidgets;
use relm4::once_cell::sync::Lazy;
use relm4::view;

use std::sync::Arc;
use tracing;
use url::Url;

use crate::duplicates_page::{DuplicatesPage, DuplicatesPageInit, DuplicatesPageOutput};
use crate::file_list::{FileList, FileListInput};
use crate::settings_dialog::{SettingsDialog, SettingsDialogOutput};
use archive_organizer::api::{File, FileDataSource};

const COMPONENT_CSS: &str = include_str!("../assets/style.css");

/// The initializer for the CSS, ensuring it only happens once.
static INITIALIZE_CSS: Lazy<()> = Lazy::new(|| {
    relm4::set_global_css_with_priority(COMPONENT_CSS, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
});

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileListSelector {
    LocalFiles,
    RemoteFiles(Url),
    DuplicatesLocal,
    DuplicatesRemote(Url),
}

pub struct App {
    application_module: ApplicationModule,
    local_file_list: AsyncController<FileList<DbClient>>,
    remote_file_lists: IndexMap<Url, AsyncController<FileList<FilesClient>>>,
    file_list_selector: FileListSelector,
    settings_dialog: Option<AsyncController<SettingsDialog>>,
    // Duplicates pages
    duplicates_local: Option<AsyncController<DuplicatesPage<DbClient>>>,
    duplicates_remote: IndexMap<Url, AsyncController<DuplicatesPage<FilesClient>>>,
}

impl App {
    pub fn get_file_list(&self) -> &gtk::Widget {
        match &self.file_list_selector {
            FileListSelector::LocalFiles => self.local_file_list.widget().upcast_ref(),
            FileListSelector::RemoteFiles(url_selector) => self
                .remote_file_lists
                .iter()
                .find(|(base_url, _)| base_url == &url_selector)
                .unwrap()
                .1
                .widget()
                .upcast_ref(),
            FileListSelector::DuplicatesLocal => self
                .duplicates_local
                .as_ref()
                .unwrap()
                .widget()
                .upcast_ref(),
            FileListSelector::DuplicatesRemote(url_selector) => self
                .duplicates_remote
                .iter()
                .find(|(base_url, _)| base_url == &url_selector)
                .unwrap()
                .1
                .widget()
                .upcast_ref(),
        }
    }
}

#[relm4::component(pub, async)]
impl AsyncComponent for App {
    type Init = ApplicationModule;
    type Input = AppMessage;
    type Output = ();
    type CommandOutput = ();

    view! {
        #[root]
        gtk::ApplicationWindow {
            set_title: Some("Archive Organizer"),
            set_default_width: 900,
            set_default_height: 700,
            set_icon_name: Some("folder-documents-symbolic"),
            set_resizable: true,

            #[wrap(Some)]
            set_titlebar = &gtk::HeaderBar {
                set_show_title_buttons: true,
                set_title_widget: Some(&gtk::Label::new(Some("Archive Organizer"))),

                pack_end = &gtk::MenuButton {
                    set_icon_name: "open-menu-symbolic",
                    set_tooltip_text: Some("Menu"),
                    set_menu_model: Some(&{
                        let menu = gtk::gio::Menu::new();
                        let settings_item = gtk::gio::MenuItem::new(Some("Settings"), Some("app.settings"));
                        menu.append_item(&settings_item);
                        menu
                    }),
                },
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_hexpand: true,
                set_vexpand: true,

                #[local_ref]
                notebook -> gtk::Notebook {
                    set_hexpand: true,
                    set_vexpand: true,
                    set_scrollable: true,
                    set_show_border: false,
                    set_show_tabs: true,
                    connect_switch_page[sender] => move |_, _, page_num| {
                        // Use if let to handle the error case gracefully
                        // This prevents panic when closing the app
                        if let Err(e) = sender.input_sender().send(AppMessage::TabChanged(page_num as usize)) {
                            // Just log the error and continue
                            tracing::debug!("Failed to send tab changed message: {e:?}");
                        }
                    },
                }
            },
        }
    }

    fn init_loading_widgets(root: Self::Root) -> Option<LoadingWidgets> {
        view! {
            #[local]
            root {
                set_title: Some("Archive Organizer"),
                set_default_size: (800, 600),

                #[name(spinner)]
                gtk::Spinner {
                    start: (),
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,
                }
            }
        }
        Some(LoadingWidgets::new(root, spinner))
    }

    async fn init(
        application_module: Self::Init,
        root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        tracing::debug!("Initializing App component");

        // Initialize the CSS.
        #[allow(clippy::no_effect)]
        *INITIALIZE_CSS;

        // Get the database client from the application module
        let db_client = application_module.db_client();

        // Get remote clients from the application module
        let remote_clients = crate::get_remote_clients(&application_module).unwrap_or_else(|e| {
            tracing::error!("Failed to get remote clients: {}", e);
            Vec::new()
        });

        // Initialize local file list with error handling
        tracing::debug!("Initializing local file list");
        let local_file_list = FileList::builder()
            .launch((
                db_client,
                application_module.settings.clone(),
                FileListSelector::LocalFiles,
            ))
            .forward(sender.input_sender(), |msg| match msg {
                crate::file_box::FileBoxOutput::OpenDuplicatesTab(selector, duplicates) => {
                    AppMessage::OpenDuplicatesTab(selector, duplicates)
                }
                _ => AppMessage::ChangeFileList,
            });
        tracing::debug!("Successfully initialized local file list");

        tracing::debug!("Initializing remote file lists");
        let mut remote_file_lists: IndexMap<Url, AsyncController<FileList<FilesClient>>> =
            IndexMap::new();
        for remote in remote_clients {
            tracing::debug!("Initializing remote file list for: {}", &remote.base_url);
            let url = remote.base_url.clone();
            let url_clone = url.clone();
            let controller = FileList::builder()
                .launch((
                    remote.clone(),
                    application_module.settings.clone(),
                    FileListSelector::RemoteFiles(url_clone),
                ))
                .forward(sender.input_sender(), |msg| match msg {
                    crate::file_box::FileBoxOutput::OpenDuplicatesTab(selector, duplicates) => {
                        AppMessage::OpenDuplicatesTab(selector, duplicates)
                    }
                    _ => AppMessage::ChangeFileList,
                });
            tracing::debug!("Successfully initialized remote file list for: {}", &url);
            remote_file_lists.insert(url, controller);
        }

        // Create notebook for tabs
        tracing::debug!("Creating notebook");
        let notebook = gtk::Notebook::new();
        notebook.set_scrollable(true);
        notebook.set_show_border(false);

        // Add local files tab
        tracing::debug!("Adding local files tab");
        let local_label = gtk::Label::new(Some("Local Files"));
        let local_widget = local_file_list.widget();
        notebook.append_page(local_widget, Some(&local_label));

        // Add remote files tabs
        tracing::debug!("Adding remote files tabs");
        for (url, remote_controller) in remote_file_lists.iter() {
            let remote_label = gtk::Label::new(Some(&format!(
                "Remote: {}",
                url.host_str().unwrap_or("Unknown")
            )));
            let remote_widget = remote_controller.widget();
            notebook.append_page(remote_widget, Some(&remote_label));
        }

        tracing::debug!("Showing notebook");
        notebook.show();

        // Create actions
        let settings_action = gtk::gio::SimpleAction::new("settings", None);
        let sender_clone = sender.input_sender().clone();
        settings_action.connect_activate(move |_, _| {
            sender_clone.send(AppMessage::OpenSettings).unwrap();
        });

        // Add actions to the application
        let app = gtk::gio::Application::default().expect("Application not found");
        app.add_action(&settings_action);

        let model = App {
            application_module,
            local_file_list,
            remote_file_lists,
            file_list_selector: FileListSelector::LocalFiles,
            settings_dialog: None,
            duplicates_local: None,
            duplicates_remote: IndexMap::new(),
        };

        let widgets = view_output!();
        tracing::debug!("App initialization complete");

        AsyncComponentParts { model, widgets }
    }

    async fn update(
        &mut self,
        msg: Self::Input,
        sender: AsyncComponentSender<Self>,
        root: &Self::Root,
    ) {
        match msg {
            AppMessage::ChangeFileList => {
                // Legacy message, not used anymore
            }
            AppMessage::TabChanged(tab_index) => {
                // Update the file list selector based on the selected tab
                if tab_index == 0 {
                    self.file_list_selector = FileListSelector::LocalFiles;
                } else {
                    // Get the total number of regular tabs (local + remote)
                    let regular_tabs_count = 1 + self.remote_file_lists.len();

                    // Check if this is a regular tab or a duplicates tab
                    if tab_index < regular_tabs_count {
                        // This is a regular file list tab
                        let remote_index = tab_index - 1;
                        if let Some((url, _)) = self.remote_file_lists.get_index(remote_index) {
                            self.file_list_selector = FileListSelector::RemoteFiles(url.clone());
                        }
                    } else {
                        // This is a duplicates tab
                        // Calculate the index within the duplicates tabs
                        let duplicates_index = tab_index - regular_tabs_count;

                        // Check if it's the local duplicates tab
                        if duplicates_index == 0 && self.duplicates_local.is_some() {
                            self.file_list_selector = FileListSelector::DuplicatesLocal;
                        } else {
                            // It's a remote duplicates tab
                            let remote_duplicates_index = if self.duplicates_local.is_some() {
                                duplicates_index - 1
                            } else {
                                duplicates_index
                            };

                            if let Some((url, _)) =
                                self.duplicates_remote.get_index(remote_duplicates_index)
                            {
                                self.file_list_selector =
                                    FileListSelector::DuplicatesRemote(url.clone());
                            }
                        }
                    }
                }
            }
            AppMessage::OpenSettings => {
                // Create and show the settings dialog
                let settings_dialog = SettingsDialog::builder()
                    .launch(self.application_module.settings.clone())
                    .forward(sender.input_sender(), |msg| match msg {
                        SettingsDialogOutput::Closed => AppMessage::SettingsDialogClosed,
                        SettingsDialogOutput::SettingsSaved(settings) => {
                            AppMessage::SettingsSaved(settings)
                        }
                    });

                self.settings_dialog = Some(settings_dialog);
            }
            AppMessage::SettingsDialogClosed => {
                // Clean up the settings dialog
                self.settings_dialog = None;
            }
            AppMessage::SettingsSaved(new_settings) => {
                // Update the application module with the new settings
                // Create a new ApplicationModule with the new settings
                let settings_copy = (*new_settings).clone();
                self.application_module = ApplicationModule::from_settings(settings_copy);

                // Update the file lists with the new settings
                self.local_file_list
                    .sender()
                    .send(crate::file_list::FileListInput::UpdateSettings(
                        self.application_module.settings.clone(),
                    ))
                    .unwrap();

                for (_, controller) in &self.remote_file_lists {
                    controller
                        .sender()
                        .send(crate::file_list::FileListInput::UpdateSettings(
                            self.application_module.settings.clone(),
                        ))
                        .unwrap();
                }

                // Clean up the settings dialog
                self.settings_dialog = None;
            }
            AppMessage::OpenDuplicatesTab(selector, duplicates) => {
                tracing::debug!(
                    "Received OpenDuplicatesTab message with {} duplicate groups for selector: {:?}",
                    duplicates.len(),
                    selector
                );
                // Get the notebook widget
                tracing::debug!("Looking for notebook widget");
                let notebook_opt = root.first_child();
                tracing::debug!("First child found: {}", notebook_opt.is_some());

                if let Some(first_child) = notebook_opt {
                    tracing::debug!("First child type: {}", first_child.type_().name());

                    // Try to get the Box inside the ApplicationWindow
                    if first_child.type_().name() == "GtkBox" {
                        // If the first child is a Box, look for the Notebook inside it
                        let box_children = first_child.first_child();
                        tracing::debug!("Box has children: {}", box_children.is_some());

                        if let Some(notebook_widget) =
                            box_children.and_then(|child| child.downcast::<gtk::Notebook>().ok())
                        {
                            tracing::debug!("Found notebook widget inside Box");
                            // We found the notebook
                            match selector {
                                FileListSelector::LocalFiles => {
                                    // Create a new duplicates page for local files
                                    let db_client = self.application_module.db_client();
                                    let init = DuplicatesPageInit {
                                        duplicates,
                                        file_data_source: db_client.clone(),
                                    };

                                    // Get the display name for the tab label
                                    let display_name = db_client.display_name();

                                    let duplicates_controller = DuplicatesPage::builder()
                                        .launch(init)
                                        .forward(sender.input_sender(), |msg| match msg {
                                            DuplicatesPageOutput::Close => {
                                                AppMessage::CloseDuplicatesTab(
                                                    FileListSelector::DuplicatesLocal,
                                                )
                                            }
                                            DuplicatesPageOutput::FileDeleted => {
                                                AppMessage::DuplicatesFileDeleted(
                                                    FileListSelector::DuplicatesLocal,
                                                )
                                            }
                                            DuplicatesPageOutput::Refreshed => {
                                                AppMessage::RefreshDuplicatesTab(
                                                    FileListSelector::DuplicatesLocal,
                                                )
                                            }
                                        });

                                    // Create a tab label with a close button
                                    let tab_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);

                                    // Add the label
                                    let label = gtk::Label::new(Some(&format!(
                                        "Duplicates: {display_name}",
                                    )));
                                    tab_box.append(&label);

                                    // Add the close button
                                    let close_button = gtk::Button::new();
                                    close_button.set_icon_name("window-close-symbolic");
                                    close_button.set_tooltip_text(Some("Close"));
                                    close_button.add_css_class("flat");
                                    close_button.add_css_class("circular");
                                    close_button.set_valign(gtk::Align::Center);

                                    // Connect the close button to the CloseDuplicatesTab message
                                    let sender_clone = sender.clone();
                                    close_button.connect_clicked(move |_| {
                                        sender_clone.input(AppMessage::CloseDuplicatesTab(
                                            FileListSelector::DuplicatesLocal,
                                        ));
                                    });

                                    tab_box.append(&close_button);
                                    tab_box.show();

                                    // Add the page to the notebook with our custom tab label
                                    notebook_widget.append_page(
                                        duplicates_controller.widget(),
                                        Some(&tab_box),
                                    );

                                    // Store the controller
                                    self.duplicates_local = Some(duplicates_controller);

                                    // Switch to the new tab
                                    let new_tab_index = notebook_widget.n_pages() - 1;
                                    notebook_widget.set_current_page(Some(new_tab_index));
                                }
                                FileListSelector::RemoteFiles(url) => {
                                    // Create a new duplicates page for remote files
                                    // Find the remote client
                                    let remote_clients =
                                        crate::get_remote_clients(&self.application_module)
                                            .unwrap_or_default();

                                    // Try to find the matching remote client
                                    let remote_client = remote_clients
                                        .into_iter()
                                        .find(|c| c.base_url.to_string() == url.to_string());

                                    // If we can't find the remote client, show an error and return
                                    if remote_client.is_none() {
                                        tracing::error!(
                                            "Could not find remote client for URL: {}",
                                            url
                                        );

                                        // Show an error dialog
                                        let dialog = gtk::MessageDialog::new(
                                            gtk::gio::Application::default()
                                                .and_then(|app| {
                                                    app.downcast::<gtk::Application>().ok()
                                                })
                                                .and_then(|app| app.active_window())
                                                .as_ref(),
                                            gtk::DialogFlags::MODAL,
                                            gtk::MessageType::Error,
                                            gtk::ButtonsType::Ok,
                                            format!(
                                                "Could not find remote client for URL: {url}",
                                            ),
                                        );
                                        dialog.set_title(Some("Error"));
                                        dialog.connect_response(|dialog, _| {
                                            dialog.close();
                                        });
                                        dialog.show();
                                        return;
                                    }

                                    let remote_client = remote_client.unwrap();

                                    // Get the display name for the tab label
                                    let display_name = remote_client.display_name();

                                    let init = DuplicatesPageInit {
                                        duplicates,
                                        file_data_source: remote_client,
                                    };

                                    let url_clone = url.clone();
                                    let duplicates_controller = DuplicatesPage::builder()
                                        .launch(init)
                                        .forward(sender.input_sender(), move |msg| match msg {
                                            DuplicatesPageOutput::Close => {
                                                AppMessage::CloseDuplicatesTab(
                                                    FileListSelector::DuplicatesRemote(
                                                        url_clone.clone(),
                                                    ),
                                                )
                                            }
                                            DuplicatesPageOutput::FileDeleted => {
                                                AppMessage::DuplicatesFileDeleted(
                                                    FileListSelector::DuplicatesRemote(
                                                        url_clone.clone(),
                                                    ),
                                                )
                                            }
                                            DuplicatesPageOutput::Refreshed => {
                                                AppMessage::RefreshDuplicatesTab(
                                                    FileListSelector::DuplicatesRemote(
                                                        url_clone.clone(),
                                                    ),
                                                )
                                            }
                                        });

                                    // Create a tab label with a close button
                                    let tab_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);

                                    // Add the label
                                    let label = gtk::Label::new(Some(&format!(
                                        "Duplicates: {display_name}",
                                    )));
                                    tab_box.append(&label);

                                    // Add the close button
                                    let close_button = gtk::Button::new();
                                    close_button.set_icon_name("window-close-symbolic");
                                    close_button.set_tooltip_text(Some("Close"));
                                    close_button.add_css_class("flat");
                                    close_button.add_css_class("circular");
                                    close_button.set_valign(gtk::Align::Center);

                                    // Connect the close button to the CloseDuplicatesTab message
                                    let sender_clone = sender.clone();
                                    let url_clone = url.clone();
                                    close_button.connect_clicked(move |_| {
                                        sender_clone.input(AppMessage::CloseDuplicatesTab(
                                            FileListSelector::DuplicatesRemote(url_clone.clone()),
                                        ));
                                    });

                                    tab_box.append(&close_button);
                                    tab_box.show();

                                    // Add the page to the notebook with our custom tab label
                                    notebook_widget.append_page(
                                        duplicates_controller.widget(),
                                        Some(&tab_box),
                                    );

                                    // Store the controller
                                    self.duplicates_remote
                                        .insert(url.clone(), duplicates_controller);

                                    // Switch to the new tab
                                    let new_tab_index = notebook_widget.n_pages() - 1;
                                    notebook_widget.set_current_page(Some(new_tab_index));
                                }
                                _ => {
                                    // We shouldn't get here
                                    tracing::warn!(
                                        "Attempted to open duplicates tab for a selector that is already a duplicates tab"
                                    );
                                }
                            }
                        } else {
                            tracing::warn!("Could not find notebook widget in Box");
                        }
                    } else {
                        tracing::warn!("First child is not a Box: {}", first_child.type_().name());
                    }
                } else {
                    tracing::warn!("Could not find first child of root");
                }
            }
            AppMessage::CloseDuplicatesTab(selector) => {
                // Get the notebook widget
                tracing::debug!("Looking for notebook widget to close tab");
                let notebook_opt = root.first_child();

                if let Some(first_child) = notebook_opt {
                    // Try to get the Box inside the ApplicationWindow
                    if first_child.type_().name() == "GtkBox" {
                        // If the first child is a Box, look for the Notebook inside it
                        if let Some(notebook_widget) = first_child
                            .first_child()
                            .and_then(|child| child.downcast::<gtk::Notebook>().ok())
                        {
                            match selector {
                                FileListSelector::DuplicatesLocal => {
                                    // Find the tab index
                                    if let Some(duplicates_controller) = &self.duplicates_local {
                                        for i in 0..notebook_widget.n_pages() {
                                            if let Some(page) = notebook_widget.nth_page(Some(i)) {
                                                if page == *duplicates_controller.widget() {
                                                    // Remove the page
                                                    notebook_widget.remove_page(Some(i));
                                                    break;
                                                }
                                            }
                                        }

                                        // Remove the controller
                                        self.duplicates_local = None;
                                    }
                                }
                                FileListSelector::DuplicatesRemote(url) => {
                                    // Find the tab index
                                    if let Some(duplicates_controller) =
                                        self.duplicates_remote.get(&url)
                                    {
                                        for i in 0..notebook_widget.n_pages() {
                                            if let Some(page) = notebook_widget.nth_page(Some(i)) {
                                                if page == *duplicates_controller.widget() {
                                                    // Remove the page
                                                    notebook_widget.remove_page(Some(i));
                                                    break;
                                                }
                                            }
                                        }

                                        // Remove the controller
                                        self.duplicates_remote.shift_remove(&url);
                                    }
                                }
                                _ => {
                                    // We shouldn't get here
                                    tracing::warn!(
                                        "Attempted to close a tab that is not a duplicates tab"
                                    );
                                }
                            }
                        } else {
                            tracing::warn!("Could not find notebook widget in Box");
                        }
                    } else {
                        tracing::warn!("First child is not a Box: {}", first_child.type_().name());
                    }
                } else {
                    tracing::warn!("Could not find first child of root");
                }
            }
            AppMessage::DuplicatesFileDeleted(selector) => {
                // Refresh the corresponding file list
                match selector {
                    FileListSelector::DuplicatesLocal => {
                        // Refresh the local file list
                        self.local_file_list
                            .sender()
                            .send(FileListInput::RefreshFiles)
                            .unwrap();
                    }
                    FileListSelector::DuplicatesRemote(url) => {
                        // Refresh the remote file list
                        if let Some(controller) = self.remote_file_lists.get(&url) {
                            controller
                                .sender()
                                .send(FileListInput::RefreshFiles)
                                .unwrap();
                        }
                    }
                    _ => {
                        // We shouldn't get here
                        tracing::warn!("Received file deleted message for a non-duplicates tab");
                    }
                }
            }
            AppMessage::RefreshDuplicatesTab(selector) => {
                tracing::debug!("Refreshing duplicates tab: {:?}", selector);
                // Get the notebook widget
                let notebook_opt = root.first_child();

                if let Some(first_child) = notebook_opt {
                    // Try to get the Box inside the ApplicationWindow
                    if first_child.type_().name() == "GtkBox" {
                        // If the first child is a Box, look for the Notebook inside it
                        if let Some(notebook_widget) = first_child
                            .first_child()
                            .and_then(|child| child.downcast::<gtk::Notebook>().ok())
                        {
                            // Find the tab index for the duplicates page
                            match selector {
                                FileListSelector::DuplicatesLocal => {
                                    if let Some(duplicates_controller) = &self.duplicates_local {
                                        // Find the tab index
                                        for i in 0..notebook_widget.n_pages() {
                                            if let Some(page) = notebook_widget.nth_page(Some(i)) {
                                                if page == *duplicates_controller.widget() {
                                                    // Switch to this tab to show the refreshed content
                                                    notebook_widget.set_current_page(Some(i));
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                }
                                FileListSelector::DuplicatesRemote(url) => {
                                    if let Some(duplicates_controller) =
                                        self.duplicates_remote.get(&url)
                                    {
                                        // Find the tab index
                                        for i in 0..notebook_widget.n_pages() {
                                            if let Some(page) = notebook_widget.nth_page(Some(i)) {
                                                if page == *duplicates_controller.widget() {
                                                    // Switch to this tab to show the refreshed content
                                                    notebook_widget.set_current_page(Some(i));
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                }
                                _ => {
                                    // We shouldn't get here
                                    tracing::warn!(
                                        "Received refresh message for a non-duplicates tab"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
pub enum AppMessage {
    ChangeFileList,
    TabChanged(usize),
    OpenSettings,
    SettingsDialogClosed,
    SettingsSaved(Arc<archive_organizer::settings::Settings>),
    OpenDuplicatesTab(FileListSelector, Vec<Vec<File>>),
    CloseDuplicatesTab(FileListSelector),
    DuplicatesFileDeleted(FileListSelector),
    RefreshDuplicatesTab(FileListSelector),
}
