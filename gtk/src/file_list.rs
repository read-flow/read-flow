use gtk::prelude::*;
use regex::Regex;
use relm4::RelmWidgetExt;
use relm4::Sender;
use relm4::component::AsyncComponent;
use relm4::component::AsyncComponentController;
use relm4::component::AsyncComponentParts;
use relm4::component::AsyncComponentSender;
use relm4::component::AsyncController;
use relm4::gtk;
use relm4::gtk::glib;
use relm4::once_cell::sync::Lazy;
use relm4::prelude::*;

use archive_organizer::api::File;
use archive_organizer::api::FileDataSource;
use archive_organizer::settings::Settings;
use std::sync::Arc;

use crate::file_box::FileBox;
use crate::file_box::FileBoxOutput;
use crate::file_details::{FileDetails, FileDetailsOutput};
use crate::tag_input::{TagInput, TagInputInput, TagInputOutput};
// We'll use this in a future update
// use crate::duplicates_dialog::{DuplicatesDialog, DuplicatesDialogInit, DuplicatesDialogOutput, FDS};

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
    // Reference to application settings
    settings: Arc<Settings>,
    // Search pattern for filtering files
    search_pattern: Option<String>,
    // Reference to the search entry
    search_entry: Option<gtk::SearchEntry>,
    // Whether regex search mode is enabled
    regex_search_mode: bool,
    // Compiled regex pattern (if in regex mode)
    regex_pattern: Option<Regex>,
    // Reference to the regex mode checkbox
    regex_toggle: Option<gtk::CheckButton>,
    // Tag input component for adding tags to multiple files
    bulk_tag_input: Option<AsyncController<TagInput>>,
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
    // Reference to the details dialog window
    details_dialog: Option<gtk::Window>,
    // Reference to the content area within the details dialog
    details_content_container: Option<gtk::Box>,
    // Currently selected file
    selected_file: Option<File>,
    // Currently selected file ID (for highlighting in the list)
    selected_file_id: Option<i32>,
    // Error message to display when files can't be loaded
    error_message: Option<String>,
    // Whether we're in offline mode (can't connect to server)
    is_offline: bool,
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
    SearchTextChanged(String),
    ClearSearch,
    ToggleRegexMode,
    AddTagToAllFiles(String),
    ConfirmAddTagToAllFiles(String),
    UpdateSettings(Arc<Settings>),
    FindDuplicates,
}

impl<FDS> FileList<FDS>
where
    FDS: FileDataSource + Clone + 'static,
{
    /// Shows a confirmation dialog before adding a tag to all displayed files
    fn show_tag_confirmation_dialog(
        tag: String,
        sender: Sender<FileListInput>,
        tag_input_sender: Option<Sender<TagInputInput>>,
    ) {
        // Get the active window to use as parent for the dialog
        let active_window = gtk::gio::Application::default()
            .and_then(|app| app.downcast::<gtk::Application>().ok())
            .and_then(|app| app.active_window());
        // Create a confirmation dialog with the active window as parent
        let dialog = gtk::MessageDialog::new(
            active_window.as_ref(),
            gtk::DialogFlags::MODAL,
            gtk::MessageType::Warning,
            gtk::ButtonsType::None,
            format!("Add tag \"{}\" to all displayed files?", tag),
        );

        // Set dialog title
        dialog.set_title(Some("Confirm Bulk Tag Addition"));

        // Add secondary text
        dialog.set_secondary_text(Some(
            "This action will add the tag to all files currently displayed in the list.",
        ));

        // Set dialog title
        dialog.set_title(Some("Confirm Bulk Tag Addition"));

        // Explicitly add a warning icon that works across desktop environments
        let header_bar = gtk::HeaderBar::new();

        // Create warning icon
        let warning_icon = gtk::Image::from_icon_name("dialog-warning");
        warning_icon.set_pixel_size(32);
        warning_icon.add_css_class("warning-icon");

        // Create title label
        let title_label = gtk::Label::new(Some("Confirm Bulk Tag Addition"));
        title_label.add_css_class("title");
        title_label.set_hexpand(true);
        title_label.set_halign(gtk::Align::Center);

        // Add icon and title to header bar
        header_bar.pack_start(&warning_icon);
        header_bar.set_title_widget(Some(&title_label));

        // Set custom title bar with warning icon
        dialog.set_titlebar(Some(&header_bar));

        // Add buttons
        dialog.add_button("Cancel", gtk::ResponseType::Cancel);
        dialog.add_button("Add Tag", gtk::ResponseType::Accept);

        // Set default response
        dialog.set_default_response(gtk::ResponseType::Accept);

        // Make the dialog modal
        dialog.set_modal(true);

        // Show the dialog and handle the response
        let tag_clone = tag.clone();

        dialog.connect_response(move |dialog, response| {
            dialog.close();

            if response == gtk::ResponseType::Accept {
                // User confirmed, proceed with adding the tag
                tracing::debug!(
                    "User confirmed adding tag '{}' to all displayed files",
                    tag_clone
                );

                // Show a loading indicator
                if let Some(input_sender) = &tag_input_sender {
                    input_sender.send(TagInputInput::SetLoading(true)).unwrap();
                }

                // Send a message to actually add the tag
                sender
                    .send(FileListInput::ConfirmAddTagToAllFiles(tag_clone.clone()))
                    .unwrap();
            } else {
                // User cancelled
                tracing::debug!("User cancelled adding tag to files");

                // Clear the tag input field
                if let Some(input_sender) = &tag_input_sender {
                    input_sender.send(TagInputInput::ClearEntry).unwrap();
                }
            }
        });

        // Present the dialog
        dialog.present();
    }
    // Helper method to update UI visibility based on offline state
    fn update_offline_state(&self) {
        // Update the visibility of the error message and file list
        if let Some(main_content) = &self.main_content_box {
            if let Some(content_box) = main_content.first_child() {
                // Get the second child (index 1), which is the error container
                // The first child (index 0) is the search box
                if let Some(error_container) =
                    content_box.first_child().and_then(|c| c.next_sibling())
                {
                    // Update error container visibility
                    error_container.set_visible(self.is_offline);

                    // Update file list visibility (should be the next sibling)
                    if let Some(file_list) = error_container.next_sibling() {
                        file_list.set_visible(!self.is_offline);
                    }
                }
            }
        }
    }

    // Helper method to apply filters and update the displayed files
    fn apply_filters(&mut self) {
        let mut mut_files = self.files.guard();
        mut_files.clear();

        // Apply reading status, tag include filters, tag deny filters, and search pattern
        for file in &self.all_files {
            // Check if the file has any hidden tags
            let hidden_tag_match = !self.settings.ui.contains_hidden_tag(&file.tags);

            // Skip files with hidden tags
            if !hidden_tag_match {
                continue;
            }

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

            // Check if the file matches the search pattern (if any)
            let search_match = match &self.search_pattern {
                None => true,                                // No search pattern, all files match
                Some(pattern) if pattern.is_empty() => true, // Empty pattern, all files match
                Some(pattern) => {
                    if self.regex_search_mode {
                        // Use regex matching if regex mode is enabled
                        if self.regex_pattern.is_none() {
                            // Try to compile the regex pattern if it's not already compiled
                            match Regex::new(pattern) {
                                Ok(regex) => self.regex_pattern = Some(regex),
                                Err(_) => return, // Invalid regex pattern, don't update the list
                            }
                        }

                        if let Some(regex) = &self.regex_pattern {
                            // Extract filename from path for matching
                            let filename = file.path.split('/').next_back().unwrap_or("");
                            // Match on filename, path, or tags
                            regex.is_match(filename)
                                || regex.is_match(&file.path)
                                || file.tags.iter().any(|tag| regex.is_match(tag))
                        } else {
                            true // Fallback if regex compilation failed
                        }
                    } else {
                        // Use normal string matching
                        let pattern = pattern.to_lowercase();
                        // Extract filename from path for matching
                        let filename = file
                            .path
                            .split('/')
                            .next_back()
                            .unwrap_or("")
                            .to_lowercase();
                        // Match on filename, path, or tags
                        filename.contains(&pattern)
                            || file.path.to_lowercase().contains(&pattern)
                            || file
                                .tags
                                .iter()
                                .any(|tag| tag.to_lowercase().contains(&pattern))
                    }
                }
            };

            // Only include files that match all filters
            if hidden_tag_match
                && status_match
                && tag_include_match
                && tag_deny_match
                && search_match
            {
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
        // Get available tags (tags that are not in either allow or deny lists and not hidden)
        let available_tags: Vec<&String> = self
            .all_tags
            .iter()
            .filter(|tag| {
                !self.tag_filters.contains(*tag)
                    && !self.tag_deny_filters.contains(*tag)
                    && !self.settings.ui.hidden_tags().contains(tag)
            })
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
    type Init = (FDS, Arc<Settings>);
    type Input = FileListInput;
    type Output = crate::file_box::FileBoxOutput;
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
                set_width_request: if model.filter_section_visible { model.expanded_sidebar_width } else { 16 },
                set_margin_all: 0,
                add_css_class: "sidebar",

                // Toggle button for sidebar
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 0,
                    set_margin_bottom: 0,
                    add_css_class: "toolbar",
                    set_margin_all: 8,
                    set_halign: gtk::Align::Start,

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

                    // Removed the Filters label to make the collapsed panel smaller
                },

                // Scrollable container for filter options
                gtk::ScrolledWindow {
                    set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),
                    set_min_content_height: 300,
                    set_propagate_natural_height: true,
                    set_vexpand: true,
                    set_visible: model.filter_section_visible,
                    add_css_class: "filter-scrolled-window",

                    // Filter options container (includes everything except the toggle button)
                    #[name(filter_options_container)]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 12,
                        set_margin_start: 12,
                        set_margin_end: 12,
                        set_margin_top: 12,
                        set_margin_bottom: 12,

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

                    // Bulk tag operations section
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 6,
                        add_css_class: "section-box",
                        add_css_class: "filter-section",

                        gtk::Label {
                            set_label: "Add Tag to All Displayed Files",
                            add_css_class: "caption-heading",
                            set_halign: gtk::Align::Start,
                            set_hexpand: true,
                            set_margin_bottom: 4,
                        },

                        // Tag input component placeholder (will be added in init)
                        #[name(bulk_tag_input_container)]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
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
            },

            // Main content area (files list and details panel)
            #[name(main_content_box)]
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_hexpand: true,
                set_vexpand: true,
                set_halign: gtk::Align::Fill,
                set_valign: gtk::Align::Fill,

                // Files list with potential error message
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_hexpand: true,
                    set_vexpand: true,
                    set_halign: gtk::Align::Fill,
                    set_valign: gtk::Align::Fill,

                    // Search bar at the top
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 8,
                        set_margin_all: 8,
                        set_halign: gtk::Align::Fill,
                        set_hexpand: true,
                        add_css_class: "search-container",

                        #[name(search_entry)]
                        gtk::SearchEntry {
                            set_placeholder_text: Some(if model.regex_search_mode {
                                "Search with regex (e.g., \\d+\\.pdf)"
                            } else {
                                "Search files by name, path, or tags"
                            }),
                            set_hexpand: true,
                            set_halign: gtk::Align::Fill,
                            add_css_class: "search-entry",
                            connect_search_changed[sender] => move |entry| {
                                let text = entry.text().to_string();
                                sender.input(FileListInput::SearchTextChanged(text));
                            },
                        },

                        // Regex mode checkbox
                        #[name(regex_toggle)]
                        gtk::CheckButton {
                            set_label: Some("Regular Expression"),
                            set_tooltip_text: Some("Enable regular expression search"),
                            set_active: model.regex_search_mode,
                            set_margin_start: 8,
                            set_margin_end: 8,
                            add_css_class: "regex-checkbox",
                            connect_toggled[sender] => move |_| {
                                sender.input(FileListInput::ToggleRegexMode);
                            },
                        },

                        gtk::Button {
                            set_icon_name: "edit-clear-symbolic",
                            set_tooltip_text: Some("Clear search"),
                            add_css_class: "flat",
                            add_css_class: "circular",
                            connect_clicked[sender] => move |_| {
                                sender.input(FileListInput::ClearSearch);
                            },
                            set_visible: model.search_pattern.is_some() && model.search_pattern.as_ref().is_some_and(|p| !p.is_empty()),
                        },

                        // Find Duplicates button
                        gtk::Button {
                            set_icon_name: "edit-copy-symbolic",
                            set_tooltip_text: Some("Find Duplicate Files"),
                            add_css_class: "flat",
                            add_css_class: "circular",
                            connect_clicked[sender] => move |_| {
                                sender.input(FileListInput::FindDuplicates);
                            },
                        },
                    },

                    // Error message container (only visible when there's an error)
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 12,
                        set_margin_all: 24,
                        set_halign: gtk::Align::Center,
                        set_valign: gtk::Align::Center,
                        set_hexpand: true,
                        set_vexpand: true,
                        add_css_class: "empty-state",
                        set_visible: model.is_offline,

                        gtk::Image {
                            set_icon_name: Some("network-error-symbolic"),
                            set_pixel_size: 48,
                            add_css_class: "empty-state-icon",
                        },

                        gtk::Label {
                            set_label: "Connection Error",
                            add_css_class: "empty-state-title",
                        },

                        gtk::Label {
                            set_label: model.error_message.as_deref().unwrap_or("Unable to retrieve files. The server may be unavailable or unreachable."),
                            set_wrap: true,
                            set_max_width_chars: 50,
                            add_css_class: "empty-state-description",
                        },

                        gtk::Button {
                            set_label: "Retry",
                            add_css_class: "suggested-action",
                            set_halign: gtk::Align::Center,
                            connect_clicked[sender] => move |_| {
                                sender.input(FileListInput::RefreshFiles);
                            },
                        },
                    },

                    // Scrollable files list (hidden when there's an error)
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
                        set_visible: !model.is_offline,

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
                },


            },
        },
    }

    async fn init(
        init: Self::Init,
        root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        let (file_data_source, settings) = init;
        tracing::debug!("Initializing FileList component");

        // Initialize the CSS.
        #[allow(clippy::no_effect)]
        *INITIALIZE_CSS;

        tracing::debug!("Loading files from data source");
        // Try to load files from the data source
        let (files, error_message) = match file_data_source.get_files().await {
            Ok(files) => {
                tracing::debug!("Successfully loaded {} files", files.len());
                (files, None)
            }
            Err(e) => {
                tracing::error!("Error loading files: {}", e);
                // Create a user-friendly error message
                let error_msg = format!("Could not connect to the file server: {}", e);
                (Vec::new(), Some(error_msg))
            }
        };

        // Store a copy of all files for filtering
        let all_files = files.clone();

        tracing::debug!("Setting up file list factory");
        let mut files_deque = AsyncFactoryVecDeque::builder()
            .launch(gtk::Box::default())
            .forward(sender.input_sender(), |output| match output {
                FileBoxOutput::FileClicked(file) => FileListInput::FileClicked(file),
                // We don't expect to receive this message from the FileBox component
                // This is only sent from the FileList to the App component
                FileBoxOutput::OpenDuplicatesTab(_selector, _duplicates) => {
                    tracing::warn!("Unexpected OpenDuplicatesTab message received from FileBox");
                    FileListInput::RefreshFiles
                }
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
            settings,
            search_pattern: None,
            search_entry: None,
            regex_search_mode: false,
            regex_pattern: None,
            regex_toggle: None,
            bulk_tag_input: None,
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
            details_dialog: None,
            details_content_container: None,
            selected_file: None,
            selected_file_id: None,
            error_message: error_message.clone(),
            is_offline: error_message.is_some(),
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
        model.search_entry = Some(widgets.search_entry.clone());
        model.regex_toggle = Some(widgets.regex_toggle.clone());

        // Create and launch the bulk tag input component
        let bulk_tag_input_controller = TagInput::builder()
            .launch((
                Vec::new(),
                "Add tag to all displayed files".to_string(),
                "Add to All".to_string(),
            ))
            .forward(sender.input_sender(), |msg| match msg {
                TagInputOutput::TagAdded(tag) => FileListInput::AddTagToAllFiles(tag),
            });

        // Add the bulk tag input component to the container
        widgets
            .bulk_tag_input_container
            .append(bulk_tag_input_controller.widget());

        // Store the controller
        model.bulk_tag_input = Some(bulk_tag_input_controller);

        // Load available tags
        sender.input(FileListInput::LoadTags);

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

        AsyncComponentParts { model, widgets }
    }

    async fn update(
        &mut self,
        msg: Self::Input,
        sender: AsyncComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match msg {
            FileListInput::RefreshFiles => {
                // Reload files from the data source
                match self.file_data_source.get_files().await {
                    Ok(files) => {
                        // Update the all_files list
                        self.all_files = files.clone();

                        // Clear any previous error message and set offline state to false
                        self.error_message = None;
                        self.is_offline = false;

                        // Apply filters to update the displayed files
                        self.apply_filters();
                    }
                    Err(e) => {
                        tracing::warn!("Error refreshing files: {}", e);

                        // Set the error message and offline state
                        self.error_message =
                            Some(format!("Could not connect to the file server: {}", e));
                        self.is_offline = true;
                    }
                }

                // Update the UI based on offline state
                self.update_offline_state();

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

                // Update the scrolled window visibility
                if let Some(container) = &self.filter_options_container {
                    if let Some(scrolled_window) = container.parent() {
                        scrolled_window.set_visible(self.filter_section_visible);
                    }
                }

                // Update the sidebar width using the outer paned widget
                if let Some(paned) = &self.outer_paned {
                    if self.filter_section_visible {
                        // Expand the sidebar
                        paned.set_position(self.expanded_sidebar_width);
                    } else {
                        // Collapse the sidebar to just fit the icon button (no label)
                        paned.set_position(16);
                    }
                }

                // Update the width request of the sidebar container
                if let Some(sidebar) = &self.sidebar_container {
                    sidebar.set_width_request(if self.filter_section_visible {
                        self.expanded_sidebar_width
                    } else {
                        16
                    });
                }

                // Find the toggle button and update its icon
                if let Some(container) = &self.filter_options_container {
                    if let Some(parent) = container.parent() {
                        // The toggle button is in the first child box
                        if let Some(toggle_box) = parent.first_child() {
                            // Update the alignment of the toggle box
                            // No need to update the box widget

                            if let Some(button) = toggle_box.first_child() {
                                if let Ok(button) = button.downcast::<gtk::Button>() {
                                    let icon_name = if self.filter_section_visible {
                                        "view-restore-symbolic"
                                    } else {
                                        "view-more-symbolic"
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
                // Only try to load tags if we don't have an error message
                // (i.e., we can connect to the server)
                if self.error_message.is_none() {
                    // Load all available tags from the data source
                    match self.file_data_source.get_files_tags().await {
                        Ok(tags) => {
                            // Update the all_tags list
                            self.all_tags = tags.clone();

                            // Update both dropdowns
                            self.update_tag_dropdowns();

                            // Update the bulk tag input component
                            if let Some(bulk_tag_input) = &self.bulk_tag_input {
                                // Filter out hidden tags
                                let visible_tags: Vec<String> = tags
                                    .iter()
                                    .filter(|tag| !self.settings.ui.hidden_tags().contains(tag))
                                    .cloned()
                                    .collect();

                                bulk_tag_input
                                    .sender()
                                    .send(TagInputInput::UpdateTags(visible_tags))
                                    .unwrap();
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Error loading tags: {}", e);
                            // We don't show an error message for tags loading failure
                            // as it's less critical than file loading
                        }
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
            FileListInput::ToggleDetailsPanel => {
                // If we have a selected file, show the details dialog
                if let Some(file) = &self.selected_file {
                    sender.input(FileListInput::FileClicked(file.clone()));
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
            FileListInput::FileClicked(file) => {
                // Store the selected file
                self.selected_file = Some(file.clone());
                // Store the selected file ID for highlighting
                self.selected_file_id = Some(file.id);

                // Apply the updated filters to refresh the file list with the new selection
                self.apply_filters();

                // Close existing dialog if any
                if let Some(dialog) = self.details_dialog.take() {
                    dialog.close();
                }

                // Extract filename for display
                let filename = file.path.split('/').next_back().unwrap_or("File");

                // Create a new dialog window
                let dialog = gtk::Window::new();
                dialog.set_default_size(600, 700);
                dialog.set_modal(true);
                dialog.set_destroy_with_parent(true);
                dialog.set_transient_for(gtk::Window::NONE);
                dialog.add_css_class("details-dialog");

                // Create a headerbar
                let headerbar = gtk::HeaderBar::new();
                headerbar.set_title_widget(Some(
                    &gtk::Label::builder()
                        .label(filename)
                        .css_classes(vec!["title"])
                        .build(),
                ));
                headerbar.set_show_title_buttons(true);

                // We don't need action buttons in the headerbar as they're already in the file details component

                // Set the headerbar as the title bar
                dialog.set_titlebar(Some(&headerbar));

                // Create a content box for the dialog
                let content_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
                content_box.set_margin_all(16);
                content_box.set_hexpand(true);
                content_box.set_vexpand(true);
                dialog.set_child(Some(&content_box));

                // Create a scrolled window for the content
                let scrolled_window = gtk::ScrolledWindow::new();
                scrolled_window.set_hexpand(true);
                scrolled_window.set_vexpand(true);
                scrolled_window.set_hscrollbar_policy(gtk::PolicyType::Never);
                scrolled_window.set_vscrollbar_policy(gtk::PolicyType::Automatic);

                // Create a box for the file details content
                let details_content = gtk::Box::new(gtk::Orientation::Vertical, 0);
                details_content.set_hexpand(true);
                details_content.set_vexpand(true);
                details_content.set_margin_start(8);
                details_content.set_margin_end(8);
                details_content.set_margin_bottom(8);
                scrolled_window.set_child(Some(&details_content));

                content_box.append(&scrolled_window);

                // Create and launch the file details component
                let controller = FileDetails::builder()
                    .launch((
                        file.clone(),
                        self.file_data_source.clone(),
                        self.settings.clone(),
                    ))
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
                details_content.append(controller.widget());

                // Store references
                self.details = Some(controller);
                self.details_dialog = Some(dialog.clone());
                self.details_content_container = Some(details_content);

                // Connect the dialog's close event
                let sender_clone = sender.input_sender().clone();
                dialog.connect_close_request(move |_| {
                    sender_clone.send(FileListInput::CloseDetailsPanel).unwrap();
                    glib::Propagation::Proceed
                });

                // Show the dialog
                dialog.present();

                // Send output to notify parent components
                sender.output(FileBoxOutput::FileClicked(file)).unwrap();
            }
            FileListInput::CloseDetailsPanel => {
                // Close the details dialog
                if let Some(dialog) = self.details_dialog.take() {
                    dialog.close();
                }

                // Clear the details component
                self.details = None;
                self.details_content_container = None;

                // Clear the selected file
                self.selected_file = None;
                self.selected_file_id = None;

                // Apply the updated filters to refresh the file list without selection
                self.apply_filters();
            }
            FileListInput::SearchTextChanged(text) => {
                tracing::debug!("Search text changed: {}", text);

                // Update the search pattern
                if text.is_empty() {
                    self.search_pattern = None;
                    self.regex_pattern = None; // Clear compiled regex when search is empty
                } else {
                    self.search_pattern = Some(text);
                    self.regex_pattern = None; // Reset compiled regex when search text changes
                }

                // Update the clear button visibility
                if let Some(search_entry) = &self.search_entry {
                    if let Some(parent) = search_entry.parent() {
                        if let Some(clear_button) = parent.last_child() {
                            clear_button.set_visible(
                                self.search_pattern.is_some()
                                    && self.search_pattern.as_ref().is_some_and(|p| !p.is_empty()),
                            );
                        }
                    }
                }

                // Apply the updated filters
                self.apply_filters();
            }
            FileListInput::ClearSearch => {
                tracing::debug!("Clearing search");

                // Clear the search pattern
                self.search_pattern = None;
                self.regex_pattern = None; // Clear compiled regex

                // Clear the search entry text
                if let Some(search_entry) = &self.search_entry {
                    search_entry.set_text("");
                }

                // Update the clear button visibility
                if let Some(search_entry) = &self.search_entry {
                    if let Some(parent) = search_entry.parent() {
                        if let Some(clear_button) = parent.last_child() {
                            clear_button.set_visible(false);
                        }
                    }
                }

                // Apply the updated filters
                self.apply_filters();
            }
            FileListInput::ToggleRegexMode => {
                tracing::debug!("Toggling regex search mode");

                // Toggle regex mode
                self.regex_search_mode = !self.regex_search_mode;
                self.regex_pattern = None; // Clear compiled regex when toggling mode

                // Update the checkbox state
                if let Some(checkbox) = &self.regex_toggle {
                    checkbox.set_active(self.regex_search_mode);
                }

                // Update search entry placeholder
                if let Some(search_entry) = &self.search_entry {
                    search_entry.set_placeholder_text(Some(if self.regex_search_mode {
                        "Search with regex (e.g., \\d+\\.pdf)"
                    } else {
                        "Search files by name, path, or tags"
                    }));
                }

                // Apply the updated filters if there's a search pattern
                if self.search_pattern.is_some() {
                    self.apply_filters();
                }
            }
            FileListInput::AddTagToAllFiles(tag) => {
                tracing::debug!("Preparing to add tag '{}' to all displayed files", tag);

                // Create a confirmation dialog using a helper function
                Self::show_tag_confirmation_dialog(
                    tag.clone(),
                    sender.input_sender().clone(),
                    self.bulk_tag_input
                        .as_ref()
                        .map(|input| input.sender().clone()),
                );
            }

            FileListInput::ConfirmAddTagToAllFiles(tag) => {
                tracing::debug!("Adding tag '{}' to all displayed files", tag);

                // Show a loading indicator (should already be showing, but just in case)
                if let Some(bulk_tag_input) = &self.bulk_tag_input {
                    bulk_tag_input
                        .sender()
                        .send(TagInputInput::SetLoading(true))
                        .unwrap();
                }

                // Get all currently displayed files
                let mut displayed_files = Vec::new();

                // We need to get the files that are currently displayed
                // Since we can't access the private fields of FileBox directly,
                // we'll use the filtered files from all_files based on our current filters
                for file in &self.all_files {
                    // Apply the same filters as in apply_filters method
                    let status_match = self.status_filters.contains(&file.status);

                    let tag_include_match = if self.tag_filters.is_empty() {
                        true
                    } else {
                        file.tags.iter().any(|tag| self.tag_filters.contains(tag))
                    };

                    let tag_deny_match = if self.tag_deny_filters.is_empty() {
                        true
                    } else {
                        !file
                            .tags
                            .iter()
                            .any(|tag| self.tag_deny_filters.contains(tag))
                    };

                    let search_match = match &self.search_pattern {
                        None => true,
                        Some(pattern) if pattern.is_empty() => true,
                        Some(pattern) => {
                            if self.regex_search_mode {
                                if let Some(regex) = &self.regex_pattern {
                                    let filename = file.path.split('/').next_back().unwrap_or("");
                                    regex.is_match(filename)
                                        || regex.is_match(&file.path)
                                        || file.tags.iter().any(|tag| regex.is_match(tag))
                                } else {
                                    true
                                }
                            } else {
                                let pattern = pattern.to_lowercase();
                                let filename = file
                                    .path
                                    .split('/')
                                    .next_back()
                                    .unwrap_or("")
                                    .to_lowercase();
                                filename.contains(&pattern)
                                    || file.path.to_lowercase().contains(&pattern)
                                    || file
                                        .tags
                                        .iter()
                                        .any(|tag| tag.to_lowercase().contains(&pattern))
                            }
                        }
                    };

                    if status_match && tag_include_match && tag_deny_match && search_match {
                        displayed_files.push(file.clone());
                    }
                }

                // If there are no files displayed, show a message and return
                if displayed_files.is_empty() {
                    tracing::warn!("No files displayed to add tag to");
                    if let Some(bulk_tag_input) = &self.bulk_tag_input {
                        bulk_tag_input
                            .sender()
                            .send(TagInputInput::SetLoading(false))
                            .unwrap();
                        bulk_tag_input
                            .sender()
                            .send(TagInputInput::ClearEntry)
                            .unwrap();
                    }
                    return;
                }

                // Add the tag to each file
                let mut success_count = 0;
                let mut error_count = 0;
                for file in &displayed_files {
                    // Skip files that already have this tag
                    if file.tags.contains(&tag) {
                        continue;
                    }

                    // Add the tag to the file
                    match self
                        .file_data_source
                        .add_file_tags(file.id, vec![tag.clone()])
                        .await
                    {
                        Ok(_) => success_count += 1,
                        Err(e) => {
                            tracing::warn!("Error adding tag to file {}: {}", file.id, e);
                            error_count += 1;
                        }
                    }
                }

                // Reset the loading indicator
                if let Some(bulk_tag_input) = &self.bulk_tag_input {
                    bulk_tag_input
                        .sender()
                        .send(TagInputInput::SetLoading(false))
                        .unwrap();
                    bulk_tag_input
                        .sender()
                        .send(TagInputInput::ClearEntry)
                        .unwrap();
                }

                tracing::info!(
                    "Added tag '{}' to {} files ({} errors)",
                    tag,
                    success_count,
                    error_count
                );

                // Update the all_files list with the new tags
                for file in &mut self.all_files {
                    if !file.tags.contains(&tag)
                        && displayed_files.iter().any(|df| df.id == file.id)
                    {
                        file.tags.push(tag.clone());
                    }
                }

                // Apply filters to update the displayed files without making a network request
                self.apply_filters();

                // Make sure we're not in offline mode after a successful operation
                if success_count > 0 {
                    self.is_offline = false;
                    self.update_offline_state();
                }

                // Also refresh the tags
                sender.input(FileListInput::LoadTags);
            }
            FileListInput::UpdateSettings(new_settings) => {
                // Update the settings
                self.settings = new_settings;

                // Reload files and tags with the new settings
                sender.input(FileListInput::RefreshFiles);
            }
            FileListInput::FindDuplicates => {
                tracing::debug!("Finding duplicate files");

                // Use the to_buckets function to group files by fingerprint
                let buckets = archive_organizer::to_buckets(self.all_files.iter(), |file| {
                    file.fingerprint.clone()
                });

                // Filter for buckets with more than one file (duplicates)
                let duplicates: Vec<Vec<File>> = buckets
                    .into_iter()
                    .filter(|(_, files)| files.len() > 1)
                    .map(|(_, files)| files.into_iter().cloned().collect())
                    .collect();

                if duplicates.is_empty() {
                    // Show a message dialog if no duplicates are found
                    let dialog = gtk::MessageDialog::new(
                        gtk::gio::Application::default()
                            .and_then(|app| app.downcast::<gtk::Application>().ok())
                            .and_then(|app| app.active_window())
                            .as_ref(),
                        gtk::DialogFlags::MODAL,
                        gtk::MessageType::Info,
                        gtk::ButtonsType::Ok,
                        "No duplicate files found",
                    );
                    dialog.set_title(Some("Duplicate Files"));
                    dialog.connect_response(|dialog, _| {
                        dialog.close();
                    });
                    dialog.show();
                } else {
                    // Determine which file list this is (local or remote)
                    let type_name = std::any::type_name::<FDS>();

                    // Create the appropriate selector based on the type
                    let selector = if type_name.contains("DbClient") {
                        // This is a local file list
                        crate::app::FileListSelector::LocalFiles
                    } else if type_name.contains("FilesClient") {
                        // This is a remote file list
                        // In a real implementation, we would need to get the URL from the file data source
                        // For now, we'll use a placeholder URL
                        let url = url::Url::parse("http://example.com").unwrap();
                        crate::app::FileListSelector::RemoteFiles(url)
                    } else {
                        // Unknown type, default to local
                        crate::app::FileListSelector::LocalFiles
                    };

                    // Create a new output message to send to the parent component
                    let output = crate::file_box::FileBoxOutput::OpenDuplicatesTab(
                        selector,
                        duplicates.clone(),
                    );

                    // Send the output message to the parent component
                    tracing::debug!(
                        "Sending OpenDuplicatesTab message with {} duplicate groups",
                        duplicates.len()
                    );
                    sender.output(output).unwrap();

                    // Show a dialog with the number of duplicates
                    let dialog = gtk::MessageDialog::new(
                        gtk::gio::Application::default()
                            .and_then(|app| app.downcast::<gtk::Application>().ok())
                            .and_then(|app| app.active_window())
                            .as_ref(),
                        gtk::DialogFlags::MODAL,
                        gtk::MessageType::Info,
                        gtk::ButtonsType::Ok,
                        format!(
                            "{} groups of duplicate files found. Opening in a new tab...",
                            duplicates.len()
                        ),
                    );
                    dialog.set_title(Some("Duplicate Files"));
                    dialog.connect_response(|dialog, _| {
                        dialog.close();
                    });
                    dialog.show();
                }
            }
        }
    }
}
