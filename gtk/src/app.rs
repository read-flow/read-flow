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
        gtk::Window {
            set_title: Some("Archive Organizer"),
            set_default_width: 800,
            set_default_height: 600,
            set_icon_name: Some("folder-archives"),

            gtk::HeaderBar {
                set_show_title_buttons: true,
                set_title_widget: Some(&gtk::Label::new(Some("Archive Organizer"))),
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 12,
                set_margin_all: 12,

                #[local_ref]
                combobox -> gtk::ComboBoxText {
                    set_hexpand: true,
                    set_vexpand: false,
                    set_margin_start: 12,
                    set_margin_end: 12,
                    set_margin_top: 12,
                    set_margin_bottom: 12,
                    connect_changed[sender] => move |_| {
                        sender.input_sender().send(AppMessage::ChangeFileList).unwrap();
                    },
                },

                gtk::ScrolledWindow {
                    set_vexpand: true,
                    set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),
                    set_margin_all: 12,

                    #[local_ref]
                    file_list -> gtk::Box,
                },
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
        // Initialize the CSS.
        #[allow(clippy::no_effect)]
        *INITIALIZE_CSS;

        let local_file_list = FileList::builder()
            .launch(file_data_source)
            .forward(sender.input_sender(), |_| AppMessage::ChangeFileList);

        let remote_file_lists: IndexMap<_, _> = remote_data_sources
            .into_iter()
            .map(|remote| {
                (
                    remote.base_url.clone(),
                    FileList::builder()
                        .launch(remote)
                        .forward(sender.input_sender(), |_| AppMessage::ChangeFileList),
                )
            })
            .collect();

        let combobox = gtk::ComboBoxText::new();
        combobox.append_text("Local Files");
        for (i, _) in remote_file_lists.iter().enumerate() {
            combobox.append_text(&format!("Remote Files {}", i));
        }

        combobox.set_active(Some(0));
        combobox.show();

        let model = App {
            local_file_list,
            remote_file_lists,
            file_list_selector: FileListSelector::LocalFiles,
            combobox: combobox.clone(),
        };

        let file_list = model.local_file_list.widget();

        let widgets = view_output!();

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
                let active_index = self.combobox.active_text().unwrap().to_string();
                if active_index == "Local Files" {
                    self.file_list_selector = FileListSelector::LocalFiles;
                } else {
                    let index = active_index.replace("Remote Files ", "");
                    let url = self
                        .remote_file_lists
                        .iter()
                        .nth(index.parse::<usize>().unwrap())
                        .unwrap()
                        .0
                        .clone();
                    self.file_list_selector = FileListSelector::RemoteFiles(url);
                }
            }
        }
    }
}

#[derive(Debug)]
pub enum AppMessage {
    ChangeFileList,
}
