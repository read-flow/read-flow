use gtk::prelude::*;
use relm4::RelmWidgetExt;
use relm4::component::AsyncComponent;
use relm4::component::AsyncComponentParts;
use relm4::component::AsyncComponentSender;
use relm4::gtk;
use relm4::once_cell::sync::Lazy;
use relm4::prelude::*;

use archive_organizer::api::File;
use archive_organizer::api::FileDataSource;

use crate::file_box::FileBox;
use crate::file_box::FileBoxOutput;
use crate::file_details::{FileDetails, FileDetailsOutput};

use tracing;

const COMPONENT_CSS: &str = include_str!("../assets/style.css");

/// The initializer for the CSS, ensuring it only happens once.
static INITIALIZE_CSS: Lazy<()> = Lazy::new(|| {
    relm4::set_global_css_with_priority(COMPONENT_CSS, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
});

use archive_organizer::api::ReadingStatus;
use std::collections::HashSet;
use strum::IntoEnumIterator;

pub struct FileList<FDS>
where
    FileDetails<FDS>: relm4::component::AsyncComponent,
{
    pub(super) file_data_source: FDS,
    files: AsyncFactoryVecDeque<FileBox>,
    details: Option<AsyncController<FileDetails<FDS>>>,
    // Track which reading statuses are selected for filtering
    status_filters: HashSet<ReadingStatus>,
    // Store all files to make filtering easier
    all_files: Vec<File>,
    // References to filter checkboxes
    unread_checkbox: Option<gtk::CheckButton>,
    reading_checkbox: Option<gtk::CheckButton>,
    read_checkbox: Option<gtk::CheckButton>,
}

#[derive(Debug)]
pub enum FileListInput {
    FileClicked(File),
    RefreshFiles,
    ToggleStatusFilter(ReadingStatus),
}

impl<FDS> FileList<FDS>
where
    FDS: FileDataSource + Clone + 'static,
{
    // Helper method to apply filters and update the displayed files
    fn apply_filters(&mut self) {
        let mut mut_files = self.files.guard();
        mut_files.clear();

        // Apply reading status filters
        for file in &self.all_files {
            if self.status_filters.contains(&file.status) {
                mut_files.push_back(file.clone());
            }
        }
    }
}

#[relm4::component(pub, async)]
impl<FDS> AsyncComponent for FileList<FDS>
where
    FDS: FileDataSource + Clone + 'static,
{
    type Init = FDS;
    type Input = FileListInput;
    type Output = FileBoxOutput;
    type CommandOutput = ();

    view! {
        #[root]
    gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 12,
            set_margin_all: 12,

            // Reading status filter section
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 8,
                set_margin_bottom: 12,

                gtk::Label {
                    set_label: "Filter by Reading Status",
                    add_css_class: "heading",
                    set_halign: gtk::Align::Start,
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 12,
                    set_margin_top: 4,
                    set_margin_bottom: 8,

                    // Unread checkbox
                    #[name(unread_checkbox)]
                    gtk::CheckButton {
                        set_label: Some("Unread"),
                        set_active: model.status_filters.contains(&ReadingStatus::Unread),
                        add_css_class: "check",
                        connect_toggled[sender] => move |_| {
                            sender.input(FileListInput::ToggleStatusFilter(ReadingStatus::Unread));
                        },
                    },

                    // Reading checkbox
                    #[name(reading_checkbox)]
                    gtk::CheckButton {
                        set_label: Some("Reading"),
                        set_active: model.status_filters.contains(&ReadingStatus::Reading),
                        add_css_class: "check",
                        connect_toggled[sender] => move |_| {
                            sender.input(FileListInput::ToggleStatusFilter(ReadingStatus::Reading));
                        },
                    },

                    // Read checkbox
                    #[name(read_checkbox)]
                    gtk::CheckButton {
                        set_label: Some("Read"),
                        set_active: model.status_filters.contains(&ReadingStatus::Read),
                        add_css_class: "check",
                        connect_toggled[sender] => move |_| {
                            sender.input(FileListInput::ToggleStatusFilter(ReadingStatus::Read));
                        },
                    },

                    gtk::Box {
                        set_hexpand: true,
                    },
                },

                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                },
            },

            // Files list
            gtk::ScrolledWindow {
                set_vexpand: true,
                #[local_ref]
                files_box -> gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 8,
                },
            },
    },
    }

    async fn init(
        file_data_source: Self::Init,
        root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        tracing::debug!("Initializing FileList component");

        // Initialize the CSS.
        #[allow(clippy::no_effect)]
        *INITIALIZE_CSS;

        tracing::debug!("Loading files from data source");
        let files = match file_data_source.get_files().await {
            Ok(files) => {
                tracing::debug!("Successfully loaded {} files", files.len());
                files
            }
            Err(e) => {
                tracing::error!("Error loading files: {}", e);
                Vec::new()
            }
        };

        // Store a copy of all files for filtering
        let all_files = files.clone();

        tracing::debug!("Setting up file list factory");
        let mut files_deque = AsyncFactoryVecDeque::builder()
            .launch(gtk::Box::default())
            .forward(sender.input_sender(), |output| match output {
                FileBoxOutput::FileClicked(file) => FileListInput::FileClicked(file),
            });

        {
            let mut mut_files = files_deque.guard();
            tracing::debug!("Adding {} files to list", files.len());
            for file in files {
                mut_files.push_back(file);
            }
        }

        // By default, show all reading statuses (no filtering)
        let mut status_filters = HashSet::new();
        for status in ReadingStatus::iter() {
            status_filters.insert(status);
        }

        let model = FileList {
            file_data_source,
            files: files_deque,
            details: None,
            status_filters,
            all_files,
            unread_checkbox: None,
            reading_checkbox: None,
            read_checkbox: None,
        };

        let files_box = model.files.widget();
        let widgets = view_output!();
        tracing::debug!("FileList initialization complete");

        // Store references to the checkboxes
        let mut model = model;
        model.unread_checkbox = Some(widgets.unread_checkbox.clone());
        model.reading_checkbox = Some(widgets.reading_checkbox.clone());
        model.read_checkbox = Some(widgets.read_checkbox.clone());

        AsyncComponentParts { model, widgets }
    }

    async fn update(
        &mut self,
        msg: Self::Input,
        sender: AsyncComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match msg {
            FileListInput::FileClicked(file) => {
                if let Some(_details) = self.details.take() {
                    // The component will be dropped when the window is closed
                }
                self.details = Some(
                    FileDetails::builder()
                        .launch((file.clone(), self.file_data_source.clone()))
                        .forward(sender.input_sender(), |msg| match msg {
                            FileDetailsOutput::TagsChanged(_) => FileListInput::RefreshFiles,
                        }),
                );
                sender.output(FileBoxOutput::FileClicked(file)).unwrap();
            }
            FileListInput::RefreshFiles => {
                // Reload files from the data source
                match self.file_data_source.get_files().await {
                    Ok(files) => {
                        // Update the all_files list
                        self.all_files = files.clone();

                        // Apply filters to update the displayed files
                        self.apply_filters();
                    }
                    Err(e) => {
                        tracing::warn!("Error refreshing files: {}", e);
                    }
                }
            }
            FileListInput::ToggleStatusFilter(status) => {
                // Toggle the status in the filter set
                if self.status_filters.contains(&status) {
                    self.status_filters.remove(&status);
                } else {
                    self.status_filters.insert(status);
                }

                // Apply the updated filters
                self.apply_filters();

                tracing::debug!("filters after toggle: {:?}", &self.status_filters);
            }
        }
    }
}
