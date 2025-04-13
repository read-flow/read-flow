use archive_organizer::client::FilesClient;
use archive_organizer::db::datasource::DbClient;
use gtk::prelude::*;
use indexmap::IndexMap;
use relm4::RelmWidgetExt;
use relm4::component::AsyncComponent;
use relm4::component::AsyncComponentController;
use relm4::component::AsyncComponentParts;
use relm4::component::AsyncComponentSender;
use relm4::component::AsyncController;
use relm4::gtk;
use relm4::loading_widgets::LoadingWidgets;
use relm4::once_cell::sync::Lazy;
use relm4::view;
use tracing;
use url::Url;

use crate::file_list::FileList;

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

impl FileListSelector {
    fn get_name(&self) -> String {
        match self {
            FileListSelector::LocalFiles => "Local Files".to_string(),
            FileListSelector::RemoteFiles(url) => url.to_string(),
        }
    }
}

pub struct App {
    local_file_list: AsyncController<FileList<DbClient>>,
    remote_file_lists: IndexMap<Url, AsyncController<FileList<FilesClient>>>,
    file_list_selector: FileListSelector,
    combobox: gtk::ComboBoxText,
    notebook: gtk::Notebook,
}

impl App {
    pub fn get_file_list(&self) -> &gtk::Box {
        match &self.file_list_selector {
            FileListSelector::LocalFiles => self.local_file_list.widget(),
            FileListSelector::RemoteFiles(url_selector) => self
                .remote_file_lists
                .iter()
                .find(|(base_url, _)| base_url == &url_selector)
                .unwrap()
                .1
                .widget(),
        }
    }
}

#[relm4::component(pub, async)]
impl AsyncComponent for App {
    type Init = (DbClient, Vec<FilesClient>);
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
                        sender.input_sender().send(AppMessage::TabChanged(page_num as usize)).unwrap();
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
        (file_data_source, remote_data_sources): Self::Init,
        root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        tracing::debug!("Initializing App component");

        // Initialize the CSS.
        #[allow(clippy::no_effect)]
        *INITIALIZE_CSS;

        // Initialize local file list with error handling
        tracing::debug!("Initializing local file list");
        let local_file_list = FileList::builder()
            .launch(file_data_source.clone())
            .forward(sender.input_sender(), |_| AppMessage::ChangeFileList);
        tracing::debug!("Successfully initialized local file list");

        tracing::debug!("Initializing remote file lists");
        let mut remote_file_lists: IndexMap<Url, AsyncController<FileList<FilesClient>>> =
            IndexMap::new();
        for remote in remote_data_sources {
            tracing::debug!("Initializing remote file list for: {}", &remote.base_url);
            let url = remote.base_url.clone();
            let controller = FileList::builder()
                .launch(remote.clone())
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

        let model = App {
            local_file_list,
            remote_file_lists,
            file_list_selector: FileListSelector::LocalFiles,
            combobox: gtk::ComboBoxText::new(),
            notebook: notebook.clone(),
        };

        let widgets = view_output!();
        tracing::debug!("App initialization complete");

        AsyncComponentParts { model, widgets }
    }

    async fn update(
        &mut self,
        msg: Self::Input,
        _sender: AsyncComponentSender<Self>,
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
        }
    }
}

#[derive(Debug)]
pub enum AppMessage {
    ChangeFileList,
    TabChanged(usize),
}
