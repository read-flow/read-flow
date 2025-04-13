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

use std::collections::HashSet;
use std::sync::Arc;

use tracing;

const COMPONENT_CSS: &str = include_str!("../assets/style.css");

/// The initializer for the CSS, ensuring it only happens once.
static INITIALIZE_CSS: Lazy<()> = Lazy::new(|| {
    relm4::set_global_css_with_priority(COMPONENT_CSS, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
});

use archive_organizer::api::ReadingStatus;
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
    // Track which tags are selected for filtering
    tag_filters: HashSet<String>,
    // Store all available tags
    all_tags: Vec<String>,
    // Store all files to make filtering easier
    all_files: Vec<File>,
    // References to filter checkboxes
    unread_checkbox: Option<gtk::CheckButton>,
    reading_checkbox: Option<gtk::CheckButton>,
    read_checkbox: Option<gtk::CheckButton>,
    // Track filter section visibility
    filter_section_visible: bool,
    // Reference to the filter options container
    filter_options_container: Option<gtk::Box>,
    // Reference to the sidebar container
    sidebar_container: Option<gtk::Box>,
    // Width of the expanded sidebar
    expanded_sidebar_width: i32,
    // Reference to the tag dropdown
    tag_dropdown: Option<gtk::DropDown>,
    // Reference to the tag filters container
    tag_filters_container: Option<gtk::FlowBox>,
    // Reference to the selected tags label
    selected_tags_label: Option<gtk::Label>,
}

#[derive(Debug)]
pub enum FileListInput {
    FileClicked(File),
    RefreshFiles,
    ToggleStatusFilter(ReadingStatus),
    ToggleFilterSection,
    AddTagFilter(String),
    RemoveTagFilter(String),
    TagSelected,
    LoadTags,
}

impl<FDS> FileList<FDS>
where
    FDS: FileDataSource + Clone + 'static,
{
    // Helper method to apply filters and update the displayed files
    fn apply_filters(&mut self) {
        let mut mut_files = self.files.guard();
        mut_files.clear();

        // Apply both reading status and tag filters
        for file in &self.all_files {
            // Check if the file matches the reading status filter
            let status_match = self.status_filters.contains(&file.status);

            // Check if the file matches the tag filters (if any)
            let tag_match = if self.tag_filters.is_empty() {
                // If no tag filters are selected, all files match
                true
            } else {
                // A file matches if it has at least one of the selected tags
                file.tags.iter().any(|tag| self.tag_filters.contains(tag))
            };

            // Only include files that match both filters
            if status_match && tag_match {
                mut_files.push_back(file.clone());
            }
        }
    }

    // Helper method to update the tag filters display
    fn update_tag_filters_display(&mut self, sender: &AsyncComponentSender<Self>) {
        if let Some(container) = &self.tag_filters_container {
            // Clear existing children
            while let Some(child) = container.first_child() {
                container.remove(&child);
            }

            // Add a button for each selected tag
            for tag in &self.tag_filters.clone() {
                let tag_clone = tag.clone();
                let button = gtk::Button::builder()
                    .label(&format!("{} ×", tag))
                    .build();

                button.add_css_class("tag-button");

                // Connect the button click to remove the tag filter
                let tag_for_closure = tag_clone.clone();
                let sender = sender.clone();
                button.connect_clicked(move |_| {
                    sender.input(FileListInput::RemoveTagFilter(tag_for_closure.clone()));
                });

                container.append(&button);
            }

            // Update visibility of the "Selected Tags:" label
            if let Some(label) = &self.selected_tags_label {
                label.set_visible(!self.tag_filters.is_empty());
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
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 12,
            set_margin_all: 12,

            // Reading status filter section (sidebar)
            #[name(sidebar_container)]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 8,
                set_margin_bottom: 12,
                set_hexpand: false,
                set_vexpand: true,
                set_width_request: model.expanded_sidebar_width,

                // Toggle button for sidebar
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,
                    set_margin_bottom: 8,

                    gtk::Button {
                        set_icon_name: if model.filter_section_visible { "panel-center-symbolic" } else { "panel-left-symbolic" },
                        set_tooltip_text: Some(if model.filter_section_visible { "Hide filters" } else { "Show filters" }),
                        add_css_class: "flat",
                        connect_clicked[sender] => move |_| {
                            sender.input(FileListInput::ToggleFilterSection);
                        },
                    },
                },

                // Filter options container (includes everything except the toggle button)
                #[name(filter_options_container)]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 12,
                    set_margin_top: 4,
                    set_margin_bottom: 8,
                    set_visible: model.filter_section_visible,

                    gtk::Label {
                        set_label: "Filter by Reading Status",
                        add_css_class: "heading",
                        set_halign: gtk::Align::Start,
                        set_hexpand: true,
                        set_margin_bottom: 8,
                    },

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

                    gtk::Separator {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_margin_top: 8,
                        set_margin_bottom: 8,
                    },

                    // Tag filtering section
                    gtk::Label {
                        set_label: "Filter by Tags",
                        add_css_class: "heading",
                        set_halign: gtk::Align::Start,
                        set_hexpand: true,
                        set_margin_bottom: 8,
                    },

                    // Tag dropdown
                    #[name(tag_dropdown)]
                    gtk::DropDown {
                        set_enable_search: true,
                        set_margin_bottom: 8,
                        connect_selected_notify[sender] => move |_| {
                            sender.input(FileListInput::TagSelected);
                        },
                    },

                    // Selected tag filters display
                    #[name(selected_tags_label)]
                    gtk::Label {
                        set_label: "Selected Tags:",
                        set_halign: gtk::Align::Start,
                        set_margin_top: 4,
                        set_visible: !model.tag_filters.is_empty(),
                    },

                    // Container for tag filter buttons
                    #[name(tag_filters_container)]
                    gtk::FlowBox {
                        set_selection_mode: gtk::SelectionMode::None,
                        set_max_children_per_line: 3,
                        set_homogeneous: false,
                        set_row_spacing: 4,
                        set_column_spacing: 4,
                        set_margin_bottom: 8,
                    },

                    gtk::Separator {
                        set_orientation: gtk::Orientation::Horizontal,
                    },
                },
            },

            // Files list
            gtk::ScrolledWindow {
                set_hexpand: true,
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
            tag_filters: HashSet::new(),
            all_tags: Vec::new(),
            all_files,
            unread_checkbox: None,
            reading_checkbox: None,
            read_checkbox: None,
            filter_section_visible: true,
            filter_options_container: None,
            sidebar_container: None,
            expanded_sidebar_width: 200, // Default expanded width
            tag_dropdown: None,
            tag_filters_container: None,
            selected_tags_label: None,
        };

        // Load tags asynchronously
        sender.input(FileListInput::LoadTags);

        let files_box = model.files.widget();
        let widgets = view_output!();
        tracing::debug!("FileList initialization complete");

        // Store references to the checkboxes and containers
        let mut model = model;
        model.unread_checkbox = Some(widgets.unread_checkbox.clone());
        model.reading_checkbox = Some(widgets.reading_checkbox.clone());
        model.read_checkbox = Some(widgets.read_checkbox.clone());
        model.filter_options_container = Some(widgets.filter_options_container.clone());
        model.sidebar_container = Some(widgets.sidebar_container.clone());
        model.tag_dropdown = Some(widgets.tag_dropdown.clone());
        model.tag_filters_container = Some(widgets.tag_filters_container.clone());
        model.selected_tags_label = Some(widgets.selected_tags_label.clone());

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

                // Also refresh the tags
                sender.input(FileListInput::LoadTags);
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
            FileListInput::ToggleFilterSection => {
                // Toggle the filter section visibility
                self.filter_section_visible = !self.filter_section_visible;

                // Update the filter options visibility
                if let Some(container) = &self.filter_options_container {
                    container.set_visible(self.filter_section_visible);
                }

                // Update the sidebar width
                if let Some(sidebar) = &self.sidebar_container {
                    if self.filter_section_visible {
                        // Expand the sidebar
                        sidebar.set_width_request(self.expanded_sidebar_width);
                    } else {
                        // Collapse the sidebar to just fit the toggle button
                        sidebar.set_width_request(40);
                    }
                }

                // Find the toggle button and update its icon
                if let Some(container) = &self.filter_options_container {
                    if let Some(parent) = container.parent() {
                        // The toggle button is in the first child box
                        if let Some(toggle_box) = parent.first_child() {
                            if let Some(button) = toggle_box.first_child() {
                                if let Ok(button) = button.downcast::<gtk::Button>() {
                                    let icon_name = if self.filter_section_visible {
                                        "panel-center-symbolic"
                                    } else {
                                        "panel-left-symbolic"
                                    };
                                    button.set_icon_name(icon_name);

                                    let tooltip = if self.filter_section_visible {
                                        "Hide filters"
                                    } else {
                                        "Show filters"
                                    };
                                    button.set_tooltip_text(Some(tooltip));
                                }
                            }
                        }
                    }
                }
            }
            FileListInput::LoadTags => {
                // Load all available tags from the data source
                match self.file_data_source.get_files_tags().await {
                    Ok(tags) => {
                        // Update the all_tags list
                        self.all_tags = tags;

                        // Update the dropdown with the new tags
                        if let Some(dropdown) = &self.tag_dropdown {
                            // Create a string list model for the dropdown
                            let model = gtk::StringList::new(&[]);

                            // Add an empty item at the beginning for "Select a tag"
                            model.append("Select a tag...");

                            // Add all tags to the model
                            for tag in &self.all_tags {
                                model.append(tag);
                            }

                            // Set the model on the dropdown
                            dropdown.set_model(Some(&model));
                            dropdown.set_selected(0); // Select the first item ("Select a tag...")
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Error loading tags: {}", e);
                    }
                }
            }
            FileListInput::TagSelected => {
                // Handle tag selection from the dropdown
                if let Some(dropdown) = &self.tag_dropdown {
                    let selected = dropdown.selected();
                    if selected > 0 { // Skip the first item ("Select a tag...")
                        if let Some(model) = dropdown.model() {
                            if let Ok(string_list) = model.downcast::<gtk::StringList>() {
                                if let Some(tag_item) = string_list.string(selected) {
                                    let tag = tag_item.to_string();

                                    // Add the tag to filters if it's not already there
                                    if !self.tag_filters.contains(&tag) {
                                        sender.input(FileListInput::AddTagFilter(tag));
                                    }

                                    // Reset dropdown selection to the first item
                                    dropdown.set_selected(0);
                                }
                            }
                        }
                    }
                }
            }
            FileListInput::AddTagFilter(tag) => {
                // Add the tag to the filter set
                self.tag_filters.insert(tag.clone());

                // Update the tag filters display
                self.update_tag_filters_display(&sender);

                // Apply the updated filters
                self.apply_filters();

                tracing::debug!("Tag filters after add: {:?}", &self.tag_filters);
            }
            FileListInput::RemoveTagFilter(tag) => {
                // Remove the tag from the filter set
                self.tag_filters.remove(&tag);

                // Update the tag filters display
                self.update_tag_filters_display(&sender);

                // Apply the updated filters
                self.apply_filters();

                tracing::debug!("Tag filters after remove: {:?}", &self.tag_filters);
            }

        }
    }


}