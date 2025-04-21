use gtk::prelude::*;
use relm4::component::AsyncComponent;
use relm4::component::AsyncComponentController;
use relm4::component::AsyncComponentParts;
use relm4::component::AsyncComponentSender;
use relm4::component::AsyncController;
use relm4::gtk;

use archive_organizer::api::File;
use archive_organizer::api::FileDataSource;
use archive_organizer::settings::Settings;
use std::sync::Arc;

use crate::file_details_section::FileDetailsSection;
use crate::file_info_section::FileInfoSection;
use crate::status_radio_group::StatusRadioGroup;
use crate::tag_badge::{TagBadge, TagBadgeHandler};
use crate::tag_input::{TagInput, TagInputInput, TagInputOutput};
use crate::ui_utils;

// Implement the TagBadgeHandler trait for the FileDetailsInput sender function
#[derive(Clone)]
struct FileDetailsTagHandler {
    sender: relm4::Sender<FileDetailsInput>,
}

impl TagBadgeHandler for FileDetailsTagHandler {
    fn on_delete_tag(&self, tag: String) {
        self.sender.send(FileDetailsInput::DeleteTag(tag)).unwrap();
    }
}

pub struct FileDetails<FDS> {
    file: File,
    filename: String,
    folder: String,
    file_data_source: FDS,
    tag_container: Option<gtk::FlowBox>,
    tag_input: Option<AsyncController<TagInput>>,
    all_tags: Vec<String>,
    title_label: Option<gtk::Label>,
    status_container: Option<gtk::Box>,
    file_info_container: Option<gtk::Box>,
    file_details_container: Option<gtk::Box>,
    settings: Arc<Settings>,
}

#[derive(Debug)]
pub enum FileDetailsInput {
    Close,
    OpenFile,
    AddTag(String),
    DeleteTag(String),
    FocusTagInput,
    UpdateReadingStatus(archive_organizer::api::ReadingStatus),
    UpdateFile(File),
    LoadAvailableTags,
}

#[derive(Debug)]
pub enum FileDetailsOutput {
    TagsChanged(i32),
    TagAdded(String),
    TagRemoved(String),
    StatusChanged(archive_organizer::api::ReadingStatus),
    OpenFile,
    Closed,
    FileUpdated(File),
}

impl<FDS> FileDetails<FDS>
where
    FDS: FileDataSource + 'static,
{
    // Helper method to refresh the tags display
    async fn refresh_tags(&mut self, sender: &AsyncComponentSender<FileDetails<FDS>>) {
        // Refresh the tags display
        if let Ok(updated_file) = self.file_data_source.get_file(self.file.id).await {
            // unwrap is safe, because otherwise the tag operation would fail.
            self.file = updated_file.unwrap();

            // Clear existing tags
            if let Some(tag_container) = &self.tag_container {
                // Remove all existing children
                while let Some(child) = tag_container.first_child() {
                    tag_container.remove(&child);
                }

                // Add new tags
                // Create a tag handler with the sender
                let tag_handler = FileDetailsTagHandler {
                    sender: sender.input_sender().clone(),
                };

                for tag in &self.file.tags {
                    let badge = TagBadge::new(tag, &tag_handler);

                    // Create a FlowBoxChild to hold the badge
                    let flow_child = gtk::FlowBoxChild::new();
                    flow_child.set_child(Some(badge.widget()));
                    flow_child.set_visible(true);
                    tag_container.append(&flow_child);

                    // Make sure the tag container is visible
                    tag_container.set_visible(true);
                }
            }

            // Notify that tags have changed
            sender
                .output(FileDetailsOutput::TagsChanged(self.file.id))
                .unwrap();

            // Refresh the available tags in the dropdown
            sender.input(FileDetailsInput::LoadAvailableTags);
        }
    }
}

#[relm4::component(pub, async)]
impl<FDS> AsyncComponent for FileDetails<FDS>
where
    FDS: FileDataSource + 'static,
{
    type Init = (File, FDS, Arc<Settings>);
    type Input = FileDetailsInput;
    type Output = FileDetailsOutput;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            add_css_class: "details-panel-content",
            set_vexpand: true,

            // Modern header with prominent file actions
            gtk::Box {
                add_css_class: "details-header",
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,

                // Title and actions row
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 16,
                    set_margin_bottom: 16,

                    // File icon based on type
                    gtk::Image {
                        set_icon_name: Some("text-x-generic-symbolic"),
                        set_pixel_size: 48,
                        set_margin_end: 16,
                    },

                    // Title and path
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 4,
                        set_hexpand: true,

                        #[name(title_label)]
                        gtk::Label {
                            set_label: &model.filename,
                            add_css_class: "details-header-title",
                            set_ellipsize: gtk::pango::EllipsizeMode::End,
                            set_halign: gtk::Align::Start,
                            set_wrap: true,
                            set_wrap_mode: gtk::pango::WrapMode::WordChar,
                        },

                        gtk::Label {
                            set_label: &model.folder,
                            add_css_class: "details-header-subtitle",
                            set_halign: gtk::Align::Start,
                            set_ellipsize: gtk::pango::EllipsizeMode::Start,
                        },
                    },

                    // Action buttons
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 8,
                        set_halign: gtk::Align::End,
                        set_valign: gtk::Align::Center,

                        // Open file button (prominent)
                        gtk::Button {
                            set_label: "Open File",
                            set_icon_name: "document-open-symbolic",
                            add_css_class: "details-action-button",
                            set_tooltip_text: Some("Open this file"),
                            connect_clicked[sender] => move |_| {
                                sender.input(FileDetailsInput::OpenFile);
                            },
                        },

                        // Close button
                        gtk::Button {
                            set_icon_name: "window-close-symbolic",
                            add_css_class: "flat",
                            add_css_class: "circular",
                            set_tooltip_text: Some("Close details"),
                            connect_clicked[sender] => move |_| {
                                sender.input(FileDetailsInput::Close);
                            },
                        }
                    }
                },
            },

            // Content area
            gtk::ScrolledWindow {
                set_hexpand: true,
                set_vexpand: true,
                set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 24,
                    set_margin_start: 24,
                    set_margin_end: 24,
                    set_margin_top: 24,
                    set_margin_bottom: 24,

                    // File information section
                    gtk::Box {
                        add_css_class: "details-card",
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 16,

                        // Card header
                        gtk::Box {
                            add_css_class: "details-card-header",
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 8,

                            gtk::Image {
                                add_css_class: "details-card-icon",
                                set_icon_name: Some("document-properties-symbolic"),
                            },

                            gtk::Label {
                                add_css_class: "details-card-title",
                                set_label: "File Information",
                                set_halign: gtk::Align::Start,
                            }
                        },

                        // File info container
                        #[name(file_info_container)]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 12,
                        }
                    },

                    // Tags section
                    gtk::Box {
                        add_css_class: "details-card",
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 16,

                        // Card header
                        gtk::Box {
                            add_css_class: "details-card-header",
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 8,

                            gtk::Image {
                                add_css_class: "details-card-icon",
                                set_icon_name: Some("tag-symbolic"),
                            },

                            gtk::Label {
                                add_css_class: "details-card-title",
                                set_label: "Tags",
                                set_halign: gtk::Align::Start,
                            }
                        },

                        // Tag input component placeholder (will be added in init)
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 0,
                            set_margin_bottom: 8,
                            #[name(tag_input_container)]
                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                            },
                        },

                        // Tag container (now below the input field)
                        #[name(tag_container)]
                        gtk::FlowBox {
                            add_css_class: "details-tags-container",
                            set_selection_mode: gtk::SelectionMode::None,
                            set_max_children_per_line: 4,
                            set_row_spacing: 8,
                            set_column_spacing: 8,
                            set_homogeneous: false,
                            set_halign: gtk::Align::Fill,
                            set_hexpand: true,
                            set_vexpand: true,
                            set_visible: true,
                        },
                    },

                    // Reading status section
                    gtk::Box {
                        add_css_class: "details-card",
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 16,

                        // Card header
                        gtk::Box {
                            add_css_class: "details-card-header",
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 8,

                            gtk::Image {
                                add_css_class: "details-card-icon",
                                set_icon_name: Some("emblem-important-symbolic"),
                            },

                            gtk::Label {
                                add_css_class: "details-card-title",
                                set_label: "Reading Status",
                                set_halign: gtk::Align::Start,
                            }
                        },

                        // Status radio group
                        #[name(status_container)]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 12,
                        }
                    },

                    // File details section
                    gtk::Box {
                        add_css_class: "details-card",
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 16,

                        // Card header
                        gtk::Box {
                            add_css_class: "details-card-header",
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 8,

                            gtk::Image {
                                add_css_class: "details-card-icon",
                                set_icon_name: Some("text-x-generic-symbolic"),
                            },

                            gtk::Label {
                                add_css_class: "details-card-title",
                                set_label: "File Details",
                                set_halign: gtk::Align::Start,
                            }
                        },

                        // File details container
                        #[name(file_details_container)]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 12,
                        }
                    }

                }
            }
        }
    }

    async fn init(
        init: Self::Init,
        root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        let (file, file_data_source, settings) = init;
        let (filename, folder) = ui_utils::extract_path_components(&file.path);

        let model = FileDetails {
            file,
            filename,
            folder,
            file_data_source,
            tag_container: None,
            tag_input: None,
            all_tags: Vec::new(),
            title_label: None,
            status_container: None,
            file_info_container: None,
            file_details_container: None,
            settings,
        };

        let widgets = view_output!();

        // Create a tag handler with the sender
        let tag_handler = FileDetailsTagHandler {
            sender: sender.input_sender().clone(),
        };

        // Add tag badges
        for tag in &model.file.tags {
            let badge = TagBadge::new(tag, &tag_handler);

            // Create a FlowBoxChild to hold the badge
            let flow_child = gtk::FlowBoxChild::new();
            flow_child.set_child(Some(badge.widget()));
            flow_child.set_visible(true);
            widgets.tag_container.append(&flow_child);

            // Make sure the tag container is visible
            widgets.tag_container.set_visible(true);
        }

        // Store references to widgets in the model
        let mut model = model;
        model.tag_container = Some(widgets.tag_container.clone());
        model.title_label = Some(widgets.title_label.clone());
        model.status_container = Some(widgets.status_container.clone());
        model.file_info_container = Some(widgets.file_info_container.clone());
        model.file_details_container = Some(widgets.file_details_container.clone());

        // Create and launch the TagInput component
        let tag_input_controller = TagInput::builder()
            .launch((Vec::new(), "Add a new tag".to_string(), "Add".to_string()))
            .forward(sender.input_sender(), |msg| match msg {
                TagInputOutput::TagAdded(tag) => FileDetailsInput::AddTag(tag),
            });

        // Add the TagInput component to the container
        widgets
            .tag_input_container
            .append(tag_input_controller.widget());

        // Store the controller
        model.tag_input = Some(tag_input_controller);

        // Load available tags
        sender.input(FileDetailsInput::LoadAvailableTags);

        // Create the component instances
        let file_info_section =
            FileInfoSection::new(&model.file.type_, &model.filename, &model.folder);

        // Add the FileInfoSection to its container
        widgets
            .file_info_container
            .append(file_info_section.widget());

        // Add the FileDetailsSection to its container
        let file_details_section = FileDetailsSection::new(&model.file);
        widgets
            .file_details_container
            .append(file_details_section.widget());

        // Add the StatusRadioGroup to its container
        let status_radio_group = StatusRadioGroup::new(model.file.status, move |status| {
            sender.input(FileDetailsInput::UpdateReadingStatus(status));
        });
        widgets.status_container.append(status_radio_group.widget());

        AsyncComponentParts { model, widgets }
    }

    async fn update(
        &mut self,
        msg: Self::Input,
        sender: AsyncComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match msg {
            FileDetailsInput::Close => {
                // Notify that we're closing
                sender.output(FileDetailsOutput::Closed).unwrap();
            }
            FileDetailsInput::OpenFile => {
                // Notify that we want to open the file
                sender.output(FileDetailsOutput::OpenFile).unwrap();
            }
            FileDetailsInput::AddTag(tag) => {
                // Show a loading indicator
                if let Some(tag_input) = &self.tag_input {
                    tag_input
                        .sender()
                        .send(TagInputInput::SetLoading(true))
                        .unwrap();
                }

                // Try to add the tag
                let result = self
                    .file_data_source
                    .add_file_tags(self.file.id, vec![tag.clone()])
                    .await;

                // Reset the loading indicator
                if let Some(tag_input) = &self.tag_input {
                    tag_input
                        .sender()
                        .send(TagInputInput::SetLoading(false))
                        .unwrap();
                    tag_input.sender().send(TagInputInput::ClearEntry).unwrap();
                }

                // Handle the result and refresh the UI
                if let Err(e) = result {
                    // Log the error
                    tracing::warn!("Failed to add tag: {}", e);
                } else {
                    self.refresh_tags(&sender).await;

                    // Notify that tags have changed
                    sender
                        .output(FileDetailsOutput::TagsChanged(self.file.id))
                        .unwrap();

                    // Notify that a tag was added
                    sender
                        .output(FileDetailsOutput::TagAdded(tag.clone()))
                        .unwrap();

                    // Notify that the file was updated
                    sender
                        .output(FileDetailsOutput::FileUpdated(self.file.clone()))
                        .unwrap();
                }
            }
            FileDetailsInput::DeleteTag(tag) => {
                // Show a loading indicator
                if let Some(tag_input) = &self.tag_input {
                    tag_input
                        .sender()
                        .send(TagInputInput::SetLoading(true))
                        .unwrap();
                }

                // Try to delete the tag
                let result = self
                    .file_data_source
                    .delete_file_tags(self.file.id, vec![tag.clone()])
                    .await;

                // Reset the loading indicator
                if let Some(tag_input) = &self.tag_input {
                    tag_input
                        .sender()
                        .send(TagInputInput::SetLoading(false))
                        .unwrap();
                }

                // Handle the result and refresh the UI
                if let Err(e) = result {
                    // Log the error
                    tracing::warn!("Failed to delete tag: {}", e);
                } else {
                    self.refresh_tags(&sender).await;

                    // Notify that tags have changed
                    sender
                        .output(FileDetailsOutput::TagsChanged(self.file.id))
                        .unwrap();

                    // Notify that a tag was removed
                    sender
                        .output(FileDetailsOutput::TagRemoved(tag.clone()))
                        .unwrap();

                    // Notify that the file was updated
                    sender
                        .output(FileDetailsOutput::FileUpdated(self.file.clone()))
                        .unwrap();
                }
            }
            FileDetailsInput::FocusTagInput => {
                // Focus the tag input component
                if let Some(tag_input) = &self.tag_input {
                    tag_input.widget().grab_focus();
                }
            }
            FileDetailsInput::UpdateReadingStatus(status) => {
                // Update the file's reading status
                if self.file.status != status {
                    // Update the local model
                    self.file.status = status;

                    // Update the file in the database
                    let result = self.file_data_source.update_file(self.file.clone()).await;

                    if let Err(e) = result {
                        tracing::warn!("Failed to update reading status: {}", e);
                    } else {
                        // Notify that the file has been updated
                        sender
                            .output(FileDetailsOutput::TagsChanged(self.file.id))
                            .unwrap();

                        // Notify that the reading status has changed
                        sender
                            .output(FileDetailsOutput::StatusChanged(status))
                            .unwrap();

                        // Notify that the file was updated
                        sender
                            .output(FileDetailsOutput::FileUpdated(self.file.clone()))
                            .unwrap();
                    }
                }
            }
            FileDetailsInput::UpdateFile(file) => {
                // Update the file
                self.file = file;

                // Update the filename and folder
                let (new_filename, new_folder) = ui_utils::extract_path_components(&self.file.path);
                self.filename = new_filename;
                self.folder = new_folder;

                // Update the UI
                if let Some(title_label) = &self.title_label {
                    title_label.set_label(&self.filename);
                }

                // Update the file info container
                if let Some(file_info_container) = &self.file_info_container {
                    // Clear the container
                    while let Some(child) = file_info_container.first_child() {
                        file_info_container.remove(&child);
                    }

                    // Add a new FileInfoSection
                    let file_info_section =
                        FileInfoSection::new(&self.file.type_, &self.filename, &self.folder);
                    file_info_container.append(file_info_section.widget());
                }

                // Update the file details container
                if let Some(file_details_container) = &self.file_details_container {
                    // Clear the container
                    while let Some(child) = file_details_container.first_child() {
                        file_details_container.remove(&child);
                    }

                    // Add a new FileDetailsSection
                    let file_details_section = FileDetailsSection::new(&self.file);
                    file_details_container.append(file_details_section.widget());
                }

                // Update the status radio group if the status has changed
                if let Some(status_container) = &self.status_container {
                    // Clear the container and add a new StatusRadioGroup
                    while let Some(child) = status_container.first_child() {
                        status_container.remove(&child);
                    }

                    // Create a new status radio group with the updated status
                    let sender_clone = sender.clone();
                    let status_radio_group =
                        StatusRadioGroup::new(self.file.status, move |status| {
                            sender_clone.input(FileDetailsInput::UpdateReadingStatus(status));
                        });
                    status_container.append(status_radio_group.widget());
                }

                // Refresh the tags
                self.refresh_tags(&sender).await;
            }

            FileDetailsInput::LoadAvailableTags => {
                // Load all available tags
                if let Ok(tags) = self.file_data_source.get_files_tags().await {
                    self.all_tags = tags.clone();

                    // Filter out tags that are already applied to this file and hidden tags
                    let current_file_tags: std::collections::HashSet<&String> =
                        self.file.tags.iter().collect();
                    let available_tags: Vec<String> = self
                        .all_tags
                        .iter()
                        .filter(|tag|
                            !current_file_tags.contains(tag) &&
                            !self.settings.ui.hidden_tags().contains(tag)
                        )
                        .cloned()
                        .collect();

                    // Update the TagInput component with the available tags
                    if let Some(tag_input) = &self.tag_input {
                        tag_input
                            .sender()
                            .send(TagInputInput::UpdateTags(available_tags))
                            .unwrap();
                    }
                }
            }
        }
    }
}
