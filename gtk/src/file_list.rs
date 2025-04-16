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
    // Track which tags are selected for filtering (include)
    tag_filters: HashSet<String>,
    // Track which tags are selected for exclusion (deny)
    tag_deny_filters: HashSet<String>,
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
    // Reference to the tag dropdown (include)
    tag_dropdown: Option<gtk::DropDown>,
    // Reference to the tag filters container (include)
    tag_filters_container: Option<gtk::FlowBox>,
    // Reference to the selected tags label (include)
    selected_tags_label: Option<gtk::Label>,
    // Reference to the tag deny dropdown
    tag_deny_dropdown: Option<gtk::DropDown>,
    // Reference to the tag deny filters container
    tag_deny_filters_container: Option<gtk::FlowBox>,
    // Reference to the selected deny tags label
    selected_deny_tags_label: Option<gtk::Label>,
    // Reference to the main content box
    main_content_box: Option<gtk::Box>,
    // Reference to the outer paned widget (for resizable left panel)
    outer_paned: Option<gtk::Paned>,
    // Reference to the inner paned widget (will be created in init)
    inner_paned: Option<gtk::Paned>,
    // Reference to the details side panel container
    details_panel_container: Option<gtk::Box>,
    // Reference to the content area within the details panel
    details_content_container: Option<gtk::Box>,
    // Track if the details panel is visible
    details_panel_visible: bool,
    // Width of the details panel
    details_panel_width: i32,
    // Last position of the main content paned before showing details
    last_paned_position: i32,
    // Currently selected file
    selected_file: Option<File>,
    // Currently selected file ID (for highlighting in the list)
    selected_file_id: Option<i32>,
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
    AddTagDenyFilter(String),
    RemoveTagDenyFilter(String),
    TagDenySelected,
    ToggleDetailsPanel,
    CloseDetailsPanel,
    OpenSelectedFile,
    AddTagToFile(String),
    RemoveTagFromFile(String),
    UpdateFileReadingStatus(ReadingStatus),
}

impl<FDS> FileList<FDS>
where
    FDS: FileDataSource + Clone + 'static,
{
    // Helper method to apply filters and update the displayed files
    fn apply_filters(&mut self) {
        let mut mut_files = self.files.guard();
        mut_files.clear();

        // Apply reading status, tag include filters, and tag deny filters
        for file in &self.all_files {
            // Check if the file matches the reading status filter
            let status_match = self.status_filters.contains(&file.status);

            // Check if the file matches the tag include filters (if any)
            let tag_include_match = if self.tag_filters.is_empty() {
                // If no tag include filters are selected, all files match
                true
            } else {
                // A file matches if it has at least one of the selected tags
                file.tags.iter().any(|tag| self.tag_filters.contains(tag))
            };

            // Check if the file matches the tag deny filters (if any)
            let tag_deny_match = if self.tag_deny_filters.is_empty() {
                // If no tag deny filters are selected, all files match
                true
            } else {
                // A file matches if it does NOT have any of the denied tags
                !file
                    .tags
                    .iter()
                    .any(|tag| self.tag_deny_filters.contains(tag))
            };

            // Only include files that match all filters
            if status_match && tag_include_match && tag_deny_match {
                mut_files.push_back((file.clone(), self.selected_file_id));
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

            // Add a chip for each selected tag
            for tag in &self.tag_filters.clone() {
                let tag_clone = tag.clone();

                // Create a chip-style container for the tag
                let chip = gtk::Box::new(gtk::Orientation::Horizontal, 4);
                chip.add_css_class("tag-chip");

                // Add the tag label
                let label = gtk::Label::new(Some(tag));
                label.add_css_class("tag-label");
                chip.append(&label);

                // Add the remove button
                let remove_btn = gtk::Button::new();
                remove_btn.set_icon_name("window-close-symbolic");
                remove_btn.add_css_class("flat");
                remove_btn.add_css_class("circular");
                remove_btn.add_css_class("tag-remove");

                // Connect the remove button
                let tag_for_closure = tag_clone.clone();
                let sender = sender.clone();
                remove_btn.connect_clicked(move |_| {
                    sender.input(FileListInput::RemoveTagFilter(tag_for_closure.clone()));
                });

                chip.append(&remove_btn);

                // Add the chip to the container
                let child = gtk::FlowBoxChild::new();
                child.set_child(Some(&chip));
                container.append(&child);
            }

            // Update visibility of the "Selected Tags:" label
            if let Some(label) = &self.selected_tags_label {
                label.set_visible(!self.tag_filters.is_empty());
            }
        }
    }

    // Helper method to update the tag deny filters display
    fn update_tag_deny_filters_display(&mut self, sender: &AsyncComponentSender<Self>) {
        if let Some(container) = &self.tag_deny_filters_container {
            // Clear existing children
            while let Some(child) = container.first_child() {
                container.remove(&child);
            }

            // Add a chip for each selected deny tag
            for tag in &self.tag_deny_filters.clone() {
                let tag_clone = tag.clone();

                // Create a chip-style container for the tag
                let chip = gtk::Box::new(gtk::Orientation::Horizontal, 4);
                chip.add_css_class("tag-chip");
                chip.add_css_class("deny"); // Additional class for styling deny tags

                // Add the tag label
                let label = gtk::Label::new(Some(tag));
                label.add_css_class("tag-label");
                chip.append(&label);

                // Add the remove button
                let remove_btn = gtk::Button::new();
                remove_btn.set_icon_name("window-close-symbolic");
                remove_btn.add_css_class("flat");
                remove_btn.add_css_class("circular");
                remove_btn.add_css_class("tag-remove");

                // Connect the remove button
                let tag_for_closure = tag_clone.clone();
                let sender = sender.clone();
                remove_btn.connect_clicked(move |_| {
                    sender.input(FileListInput::RemoveTagDenyFilter(tag_for_closure.clone()));
                });

                chip.append(&remove_btn);

                // Add the chip to the container
                let child = gtk::FlowBoxChild::new();
                child.set_child(Some(&chip));
                container.append(&child);
            }

            // Update visibility of the "Excluded Tags:" label
            if let Some(label) = &self.selected_deny_tags_label {
                label.set_visible(!self.tag_deny_filters.is_empty());
            }
        }
    }

    // Helper method to update both tag dropdowns with available tags
    fn update_tag_dropdowns(&self) {
        // Get available tags (tags that are not in either allow or deny lists)
        let available_tags: Vec<&String> = self
            .all_tags
            .iter()
            .filter(|tag| !self.tag_filters.contains(*tag) && !self.tag_deny_filters.contains(*tag))
            .collect();

        // Update the include dropdown
        if let Some(dropdown) = &self.tag_dropdown {
            // Create a string list model for the dropdown
            let model = gtk::StringList::new(&[]);

            // Add an empty item at the beginning for "Select a tag"
            model.append("Select a tag...");

            // Add available tags to the model
            for tag in &available_tags {
                model.append(tag);
            }

            // Set the model on the dropdown
            dropdown.set_model(Some(&model));
            dropdown.set_selected(0); // Select the first item
        }

        // Update the deny dropdown
        if let Some(dropdown) = &self.tag_deny_dropdown {
            // Create a string list model for the dropdown
            let model = gtk::StringList::new(&[]);

            // Add an empty item at the beginning for "Select a tag to exclude"
            model.append("Select a tag to exclude...");

            // Add available tags to the model
            for tag in &available_tags {
                model.append(tag);
            }

            // Set the model on the dropdown
            dropdown.set_model(Some(&model));
            dropdown.set_selected(0); // Select the first item
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
            set_spacing: 0,
            set_margin_all: 0,
            set_hexpand: true,
            set_vexpand: true,
            set_halign: gtk::Align::Fill,
            set_valign: gtk::Align::Fill,

            // Left side: sidebar with filters
            #[name(sidebar_container)]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                set_hexpand: false,
                set_vexpand: true,
                set_width_request: model.expanded_sidebar_width,
                add_css_class: "sidebar",

                // Toggle button for sidebar
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 0,
                    set_margin_bottom: 0,
                    add_css_class: "toolbar",
                    set_margin_all: 8,

                    gtk::Button {
                        set_icon_name: if model.filter_section_visible { "view-restore-symbolic" } else { "view-more-symbolic" },
                        set_tooltip_text: Some(if model.filter_section_visible { "Hide filters" } else { "Show filters" }),
                        add_css_class: "flat",
                        add_css_class: "circular",
                        add_css_class: "filter-toggle",
                        connect_clicked[sender] => move |_| {
                            sender.input(FileListInput::ToggleFilterSection);
                        },
                    },

                    gtk::Label {
                        set_label: "Filters",
                        add_css_class: "heading",
                        set_halign: gtk::Align::Start,
                        set_hexpand: true,
                        set_margin_start: 8,
                        set_visible: model.filter_section_visible,
                    },
                },

                // Filter options container (includes everything except the toggle button)
                #[name(filter_options_container)]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 12,
                    set_margin_start: 12,
                    set_margin_end: 12,
                    set_margin_top: 12,
                    set_margin_bottom: 12,
                    set_visible: model.filter_section_visible,

                    // Reading Status Section
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 6,  // Reduced spacing
                        add_css_class: "section-box",

                        gtk::Label {
                            set_label: "Reading Status",
                            add_css_class: "caption-heading",
                            set_halign: gtk::Align::Start,
                            set_hexpand: true,
                            set_margin_bottom: 4,  // Reduced margin
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
                    },

                    // Tag filtering section
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 6,  // Reduced spacing
                        add_css_class: "section-box",
                        add_css_class: "filter-section",

                        gtk::Label {
                            set_label: "Include Files with Tags",
                            add_css_class: "caption-heading",
                            set_halign: gtk::Align::Start,
                            set_hexpand: true,
                            set_margin_bottom: 4,  // Reduced margin
                        },

                        // Tag dropdown with improved styling
                        #[name(tag_dropdown)]
                        gtk::DropDown {
                            set_enable_search: true,
                            set_margin_bottom: 6,  // Reduced margin
                            add_css_class: "tag-dropdown",
                            connect_selected_notify[sender] => move |_| {
                                sender.input(FileListInput::TagSelected);
                            },
                        },

                        // Selected tag filters display
                        #[name(selected_tags_label)]
                        gtk::Label {
                            set_label: "Selected Tags:",
                            add_css_class: "caption-heading",
                            set_halign: gtk::Align::Start,
                            set_margin_top: 4,
                            set_visible: !model.tag_filters.is_empty(),
                        },

                        // Container for tag filter buttons
                        #[name(tag_filters_container)]
                        gtk::FlowBox {
                            set_selection_mode: gtk::SelectionMode::None,
                            set_max_children_per_line: 100,  // Allow many tags per line for better space usage
                            set_homogeneous: false,  // Don't make all children the same size
                            set_row_spacing: 2,  // Reduced spacing
                            set_column_spacing: 2,  // Reduced spacing
                            set_margin_bottom: 4,  // Reduced margin
                        },
                    },

                    // Tag deny filtering section
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 6,  // Reduced spacing
                        add_css_class: "section-box",
                        add_css_class: "filter-section",

                        gtk::Label {
                            set_label: "Exclude Files with Tags",
                            add_css_class: "caption-heading",
                            set_halign: gtk::Align::Start,
                            set_hexpand: true,
                            set_margin_bottom: 4,  // Reduced margin
                        },

                        // Tag deny dropdown with improved styling
                        #[name(tag_deny_dropdown)]
                        gtk::DropDown {
                            set_enable_search: true,
                            set_margin_bottom: 6,  // Reduced margin
                            add_css_class: "tag-dropdown",
                            connect_selected_notify[sender] => move |_| {
                                sender.input(FileListInput::TagDenySelected);
                            },
                        },

                        // Selected tag deny filters display
                        #[name(selected_deny_tags_label)]
                        gtk::Label {
                            set_label: "Excluded Tags:",
                            add_css_class: "caption-heading",
                            set_halign: gtk::Align::Start,
                            set_margin_top: 4,
                            set_visible: !model.tag_deny_filters.is_empty(),
                        },

                        // Container for tag deny filter buttons
                        #[name(tag_deny_filters_container)]
                        gtk::FlowBox {
                            set_selection_mode: gtk::SelectionMode::None,
                            set_max_children_per_line: 100,  // Allow many tags per line for better space usage
                            set_homogeneous: false,  // Don't make all children the same size
                            set_row_spacing: 2,  // Reduced spacing
                            set_column_spacing: 2,  // Reduced spacing
                            set_margin_bottom: 4,  // Reduced margin
                        },
                    },
                },
            },

            // Main content area (files list and details panel)
            #[name(main_content_box)]
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_hexpand: true,
                set_vexpand: true,
                set_halign: gtk::Align::Fill,
                set_valign: gtk::Align::Fill,

                // Files list
                gtk::ScrolledWindow {
                    set_hexpand: true,
                    set_vexpand: true,
                    set_halign: gtk::Align::Fill,
                    set_valign: gtk::Align::Fill,
                    add_css_class: "file-list-container",
                    set_margin_start: 0,
                    set_margin_end: 0,
                    set_margin_top: 0,
                    set_margin_bottom: 0,

                    #[local_ref]
                    files_box -> gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 12,  // Increased spacing between files
                        set_margin_start: 8,
                        set_margin_end: 8,
                        set_margin_top: 8,
                        set_margin_bottom: 8,
                        set_hexpand: true,
                        set_halign: gtk::Align::Fill,
                    },
                },

                // Details side panel
                #[name(details_panel_container)]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_width_request: model.details_panel_width,
                    set_visible: model.details_panel_visible,
                    set_hexpand: true,  // Changed to true to use available space
                    set_vexpand: true,
                    set_halign: gtk::Align::Fill,
                    set_valign: gtk::Align::Fill,
                    add_css_class: "sidebar",
                    add_css_class: "details-panel",

                    // Header with close button
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 0,
                        add_css_class: "toolbar",
                        set_margin_all: 8,

                        gtk::Label {
                            set_label: "File Details",
                            add_css_class: "heading",
                            set_halign: gtk::Align::Start,
                            set_hexpand: true,
                        },

                        gtk::Button {
                            set_icon_name: "window-close-symbolic",
                            add_css_class: "flat",
                            add_css_class: "circular",
                            set_tooltip_text: Some("Close details"),
                            connect_clicked[sender] => move |_| {
                                sender.input(FileListInput::CloseDetailsPanel);
                            },
                        },
                    },

                    // Scrollable content area for file details
                    gtk::ScrolledWindow {
                        set_hexpand: true,
                        set_vexpand: true,
                        set_hscrollbar_policy: gtk::PolicyType::Never,
                        set_vscrollbar_policy: gtk::PolicyType::Automatic,

                        #[name(details_content_container)]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_hexpand: true,
                            set_vexpand: true,
                            set_margin_start: 8,
                            set_margin_end: 8,
                            set_margin_bottom: 8,
                        }
                    }
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
                mut_files.push_back((file, None));
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
            tag_deny_filters: HashSet::new(),
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
            tag_deny_dropdown: None,
            tag_deny_filters_container: None,
            selected_deny_tags_label: None,
            main_content_box: None,
            outer_paned: None,
            inner_paned: None,
            details_panel_container: None,
            details_content_container: None,
            details_panel_visible: false, // Initially hidden
            details_panel_width: 300,     // Default width for details panel
            last_paned_position: 600,     // Default position before showing details
            selected_file: None,
            selected_file_id: None,
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
        model.tag_deny_dropdown = Some(widgets.tag_deny_dropdown.clone());
        model.tag_deny_filters_container = Some(widgets.tag_deny_filters_container.clone());
        model.selected_deny_tags_label = Some(widgets.selected_deny_tags_label.clone());
        model.main_content_box = Some(widgets.main_content_box.clone());
        model.details_panel_container = Some(widgets.details_panel_container.clone());
        model.details_content_container = Some(widgets.details_content_container.clone());

        // Create an outer paned widget to make the left panel resizable
        let outer_paned = gtk::Paned::new(gtk::Orientation::Horizontal);
        outer_paned.set_position(model.expanded_sidebar_width);
        outer_paned.set_resize_start_child(true);
        outer_paned.set_shrink_start_child(false);
        outer_paned.set_wide_handle(true);
        outer_paned.set_hexpand(true);
        outer_paned.set_vexpand(true);
        outer_paned.set_halign(gtk::Align::Fill);
        outer_paned.set_valign(gtk::Align::Fill);

        // Get the children from the root box
        if let Some(sidebar) = root.first_child() {
            if let Some(main_content) = root.last_child() {
                // Remove them from the box
                root.remove(&sidebar);
                root.remove(&main_content);

                // Add them to the paned widget
                outer_paned.set_start_child(Some(&sidebar));
                outer_paned.set_end_child(Some(&main_content));

                // Add the paned widget to the root
                root.append(&outer_paned);

                // Store a reference to the outer paned widget
                model.outer_paned = Some(outer_paned);
            }
        }

        // Create the inner paned widget for resizable panels
        let inner_paned = gtk::Paned::new(gtk::Orientation::Horizontal);
        inner_paned.set_position(800); // Default position
        inner_paned.set_resize_end_child(true);
        inner_paned.set_shrink_end_child(false);
        inner_paned.set_wide_handle(true);
        inner_paned.set_hexpand(true);
        inner_paned.set_vexpand(true);
        inner_paned.set_halign(gtk::Align::Fill);
        inner_paned.set_valign(gtk::Align::Fill);

        // Get the children from the main content box
        if let Some(main_box) = &model.main_content_box {
            if let Some(files_scroll) = main_box.first_child() {
                if let Some(details_panel) = main_box.last_child() {
                    // Remove them from the box
                    main_box.remove(&files_scroll);
                    main_box.remove(&details_panel);

                    // Add them to the paned widget
                    inner_paned.set_start_child(Some(&files_scroll));
                    inner_paned.set_end_child(Some(&details_panel));

                    // Add the paned widget to the main content box
                    main_box.append(&inner_paned);

                    // Store a reference to the inner paned widget
                    model.inner_paned = Some(inner_paned);
                }
            }
        }

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
                // Store the selected file
                self.selected_file = Some(file.clone());
                // Store the selected file ID for highlighting
                self.selected_file_id = Some(file.id);

                // Show the details panel
                if let Some(panel) = &self.details_panel_container {
                    if let Some(content_container) = &self.details_content_container {
                        // If the panel is already visible, we need to remove the existing component
                        if self.details_panel_visible {
                            // Remove the existing file details component
                            if let Some(_details) = self.details.take() {
                                // Remove all children from the content container
                                while let Some(child) = content_container.first_child() {
                                    content_container.remove(&child);
                                }
                            }
                        }

                        // Create and launch the file details component
                        let controller = FileDetails::builder()
                            .launch((file.clone(), self.file_data_source.clone()))
                            .forward(sender.input_sender(), |msg| match msg {
                                FileDetailsOutput::TagsChanged(_) => FileListInput::RefreshFiles,
                                FileDetailsOutput::TagAdded(_) => FileListInput::RefreshFiles,
                                FileDetailsOutput::TagRemoved(_) => FileListInput::RefreshFiles,
                                FileDetailsOutput::StatusChanged(_) => FileListInput::RefreshFiles,
                                FileDetailsOutput::FileUpdated(_) => FileListInput::RefreshFiles,
                                FileDetailsOutput::OpenFile => FileListInput::OpenSelectedFile,
                                FileDetailsOutput::Closed => FileListInput::CloseDetailsPanel,
                            });

                        // Add the component's widget to the content container
                        content_container.append(controller.widget());

                        // Store the controller
                        self.details = Some(controller);

                        // Make sure the panel is visible
                        panel.set_visible(true);
                        self.details_panel_visible = true;

                        // Adjust the inner paned position to show the details panel
                        if let Some(paned) = &self.inner_paned {
                            if !self.details_panel_visible {
                                // Store the current position
                                self.last_paned_position = paned.position();
                            }

                            // Set the position to show the details panel
                            // Calculate a position that gives the details panel its requested width
                            // but ensures it doesn't take more than 40% of the total width
                            let total_width = paned.width();
                            let max_details_width = (total_width as f64 * 0.4) as i32;
                            let details_width = self.details_panel_width.min(max_details_width);
                            let new_position = total_width - details_width;
                            paned.set_position(new_position);
                        }
                    }
                }

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

                // Update the sidebar width using the outer paned widget
                if let Some(paned) = &self.outer_paned {
                    if self.filter_section_visible {
                        // Expand the sidebar
                        paned.set_position(self.expanded_sidebar_width);
                    } else {
                        // Collapse the sidebar to just fit the toggle button
                        paned.set_position(40);
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

                        // Update both dropdowns
                        self.update_tag_dropdowns();
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
                    if selected > 0 {
                        // Skip the first item ("Select a tag...")
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

                // Update the dropdowns to remove the selected tag
                self.update_tag_dropdowns();

                // Apply the updated filters
                self.apply_filters();

                tracing::debug!("Tag filters after add: {:?}", &self.tag_filters);
            }
            FileListInput::RemoveTagFilter(tag) => {
                // Remove the tag from the filter set
                self.tag_filters.remove(&tag);

                // Update the tag filters display
                self.update_tag_filters_display(&sender);

                // Update the dropdowns to add back the removed tag
                self.update_tag_dropdowns();

                // Apply the updated filters
                self.apply_filters();

                tracing::debug!("Tag filters after remove: {:?}", &self.tag_filters);
            }
            FileListInput::TagDenySelected => {
                // Handle tag deny selection from the dropdown
                if let Some(dropdown) = &self.tag_deny_dropdown {
                    let selected = dropdown.selected();
                    if selected > 0 {
                        // Skip the first item ("Select a tag to exclude...")
                        if let Some(model) = dropdown.model() {
                            if let Ok(string_list) = model.downcast::<gtk::StringList>() {
                                if let Some(tag_item) = string_list.string(selected) {
                                    let tag = tag_item.to_string();

                                    // Add the tag to deny filters if it's not already there
                                    if !self.tag_deny_filters.contains(&tag) {
                                        sender.input(FileListInput::AddTagDenyFilter(tag));
                                    }

                                    // Reset dropdown selection to the first item
                                    dropdown.set_selected(0);
                                }
                            }
                        }
                    }
                }
            }
            FileListInput::AddTagDenyFilter(tag) => {
                // Add the tag to the deny filter set
                self.tag_deny_filters.insert(tag.clone());

                // Update the tag deny filters display
                self.update_tag_deny_filters_display(&sender);

                // Update the dropdowns to remove the selected tag
                self.update_tag_dropdowns();

                // Apply the updated filters
                self.apply_filters();

                tracing::debug!("Tag deny filters after add: {:?}", &self.tag_deny_filters);
            }
            FileListInput::RemoveTagDenyFilter(tag) => {
                // Remove the tag from the deny filter set
                self.tag_deny_filters.remove(&tag);

                // Update the tag deny filters display
                self.update_tag_deny_filters_display(&sender);

                // Update the dropdowns to add back the removed tag
                self.update_tag_dropdowns();

                // Apply the updated filters
                self.apply_filters();

                tracing::debug!(
                    "Tag deny filters after remove: {:?}",
                    &self.tag_deny_filters
                );
            }
            FileListInput::CloseDetailsPanel => {
                // Hide the details panel
                if let Some(panel) = &self.details_panel_container {
                    if let Some(content_container) = &self.details_content_container {
                        // Remove the file details component if it exists
                        if let Some(_details) = self.details.take() {
                            // The component will be dropped

                            // Remove all children from the content container
                            while let Some(child) = content_container.first_child() {
                                content_container.remove(&child);
                            }
                        }
                    }

                    panel.set_visible(false);
                    self.details_panel_visible = false;

                    // Restore the inner paned position
                    if let Some(paned) = &self.inner_paned {
                        paned.set_position(self.last_paned_position);
                    }
                }

                // Clear the selected file
                self.selected_file = None;
                // Clear the selected file ID
                self.selected_file_id = None;

                // Refresh the file list to update the highlighting
                self.apply_filters();
            }
            FileListInput::ToggleDetailsPanel => {
                // Toggle the details panel visibility
                if let Some(_panel) = &self.details_panel_container {
                    if self.details_panel_visible {
                        // If it's visible, close it
                        sender.input(FileListInput::CloseDetailsPanel);
                    } else {
                        // If it's hidden and we have a selected file, show it
                        if let Some(file) = &self.selected_file {
                            sender.input(FileListInput::FileClicked(file.clone()));
                        }
                    }
                }
            }
            FileListInput::OpenSelectedFile => {
                // Open the selected file
                if let Some(file) = &self.selected_file {
                    if let Err(e) = self.file_data_source.xdg_open_file(file.clone()).await {
                        tracing::warn!("Error opening file: {}", e);
                    }
                }
            }
            FileListInput::AddTagToFile(tag) => {
                // Add the tag to the selected file
                if let Some(file) = &mut self.selected_file {
                    if !file.tags.contains(&tag) {
                        file.tags.push(tag.clone());

                        // Update the file in the data source
                        if let Err(e) = self.file_data_source.update_file(file.clone()).await {
                            tracing::warn!("Error updating file: {}", e);
                        }

                        // Refresh the files list
                        sender.input(FileListInput::RefreshFiles);
                    }
                }
            }
            FileListInput::RemoveTagFromFile(tag) => {
                // Remove the tag from the selected file
                if let Some(file) = &mut self.selected_file {
                    if let Some(index) = file.tags.iter().position(|t| t == &tag) {
                        file.tags.remove(index);

                        // Update the file in the data source
                        if let Err(e) = self.file_data_source.update_file(file.clone()).await {
                            tracing::warn!("Error updating file: {}", e);
                        }

                        // Refresh the files list
                        sender.input(FileListInput::RefreshFiles);
                    }
                }
            }
            FileListInput::UpdateFileReadingStatus(status) => {
                // Update the reading status of the selected file
                if let Some(file) = &mut self.selected_file {
                    file.status = status;

                    // Update the file in the data source
                    if let Err(e) = self.file_data_source.update_file(file.clone()).await {
                        tracing::warn!("Error updating file: {}", e);
                    }

                    // Refresh the files list
                    sender.input(FileListInput::RefreshFiles);
                }
            }
        }
    }
}
