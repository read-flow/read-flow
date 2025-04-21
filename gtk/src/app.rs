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

use crate::file_list::FileList;
use crate::settings_dialog::{SettingsDialog, SettingsDialogOutput};

const COMPONENT_CSS: &str = include_str!("../assets/style.css");

/// The initializer for the CSS, ensuring it only happens once.
static INITIALIZE_CSS: Lazy<()> = Lazy::new(|| {
    relm4::set_global_css_with_priority(COMPONENT_CSS, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
});

#[derive(Debug, Clone, PartialEq, Eq)]
enum FileListSelector {
    LocalFiles,
    RemoteFiles(Url),
}

pub struct App {
    application_module: ApplicationModule,
    local_file_list: AsyncController<FileList<DbClient>>,
    remote_file_lists: IndexMap<Url, AsyncController<FileList<FilesClient>>>,
    file_list_selector: FileListSelector,
    settings_dialog: Option<AsyncController<SettingsDialog>>,
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
            .launch((db_client, application_module.settings.clone()))
            .forward(sender.input_sender(), |_| AppMessage::ChangeFileList);
        tracing::debug!("Successfully initialized local file list");

        tracing::debug!("Initializing remote file lists");
        let mut remote_file_lists: IndexMap<Url, AsyncController<FileList<FilesClient>>> =
            IndexMap::new();
        for remote in remote_clients {
            tracing::debug!("Initializing remote file list for: {}", &remote.base_url);
            let url = remote.base_url.clone();
            let controller = FileList::builder()
                .launch((remote.clone(), application_module.settings.clone()))
                .forward(sender.input_sender(), |_| AppMessage::ChangeFileList);
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
        };

        let widgets = view_output!();
        tracing::debug!("App initialization complete");

        AsyncComponentParts { model, widgets }
    }

    async fn update(
        &mut self,
        msg: Self::Input,
        sender: AsyncComponentSender<Self>,
        _root: &Self::Root,
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
                    let remote_index = tab_index - 1;
                    if let Some((url, _)) = self.remote_file_lists.get_index(remote_index) {
                        self.file_list_selector = FileListSelector::RemoteFiles(url.clone());
                    }
                }
            }
            AppMessage::OpenSettings => {
                // Create and show the settings dialog
                let settings_dialog = SettingsDialog::builder()
                    .launch(self.application_module.settings.clone())
                    .forward(sender.input_sender(), |msg| match msg {
                        SettingsDialogOutput::Closed => AppMessage::SettingsDialogClosed,
                        SettingsDialogOutput::SettingsSaved(settings) => AppMessage::SettingsSaved(settings),
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
                self.local_file_list.sender().send(crate::file_list::FileListInput::UpdateSettings(self.application_module.settings.clone())).unwrap();

                for (_, controller) in &self.remote_file_lists {
                    controller.sender().send(crate::file_list::FileListInput::UpdateSettings(self.application_module.settings.clone())).unwrap();
                }

                // Clean up the settings dialog
                self.settings_dialog = None;
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
}
