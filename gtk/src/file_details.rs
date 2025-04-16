use gtk::prelude::*;
use relm4::RelmWidgetExt;
use relm4::component::AsyncComponent;
use relm4::component::AsyncComponentParts;
use relm4::component::AsyncComponentSender;
use relm4::gtk;

use archive_organizer::api::File;
use archive_organizer::api::FileDataSource;

use crate::file_details_section::FileDetailsSection;
use crate::file_info_section::FileInfoSection;
use crate::status_radio_group::StatusRadioGroup;
use crate::tag_badge::{TagBadge, TagBadgeHandler};
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
    tag_input: Option<gtk::Entry>,
    title_label: Option<gtk::Label>,
    status_container: Option<gtk::Box>,
    file_info_container: Option<gtk::Box>,
    file_details_container: Option<gtk::Box>,
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
        }
    }
}

#[relm4::component(pub, async)]
impl<FDS> AsyncComponent for FileDetails<FDS>
where
    FDS: FileDataSource + 'static,
{
    type Init = (File, FDS);
    type Input = FileDetailsInput;
    type Output = FileDetailsOutput;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 8,
            add_css_class: "default-spacing",
            add_css_class: "details-panel-content",
            set_vexpand: true,

            // File header with title and open button
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                set_margin_bottom: 8,

                #[name(title_label)]
                gtk::Label {
                    set_label: &model.filename,
                    add_css_class: "title-4",
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                    set_hexpand: true,
                    set_halign: gtk::Align::Start,
                    set_wrap: true,
                    set_wrap_mode: gtk::pango::WrapMode::WordChar,
                },

                gtk::Button {
                    set_icon_name: "document-open-symbolic",
                    set_tooltip_text: Some("Open File"),
                    add_css_class: "flat",
                    add_css_class: "circular",
                    connect_clicked[sender] => move |_| {
                        sender.input(FileDetailsInput::OpenFile);
                    },
                },
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 16,

                // File information section
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 12,
                    set_margin_bottom: 8,

                    // File info container
                    #[name(file_info_container)]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 12,
                        set_margin_bottom: 8,
                    }
                },

                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                },

                // Tags section
                gtk::Box {
                    #[name(tag_container)]
                    gtk::FlowBox {
                        set_selection_mode: gtk::SelectionMode::None,
                        set_max_children_per_line: 4,  // Fewer tags per line for side panel
                        set_row_spacing: 2,  // Reduced spacing
                        set_column_spacing: 2,  // Reduced spacing
                        set_homogeneous: false,  // Don't make all children the same size
                        set_halign: gtk::Align::Fill,
                        set_hexpand: true,
                        set_vexpand: true,
                        set_margin_all: 4,
                        set_visible: true,
                        add_css_class: "tag-container",  // Add the tag-container class
                    },

                    // Tag input
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 8,
                        set_margin_bottom: 8,
                        add_css_class: "tag-input-box",
                        add_css_class: "linked",
                        set_margin_start: 4,
                        set_margin_end: 4,

                        #[name(tag_input)]
                        gtk::Entry {
                            set_placeholder_text: Some("Add a new tag"),
                            set_hexpand: true,
                            set_tooltip_text: Some("Enter a tag and press Enter to add it"),
                            set_accessible_role: gtk::AccessibleRole::SearchBox,
                            add_css_class: "search-entry",
                            connect_activate[sender] => move |entry| {
                                let tag = entry.text().as_str().trim().to_string();
                                if !tag.is_empty() {
                                    sender.input(FileDetailsInput::AddTag(tag));
                                    entry.set_text("");
                                }
                            },
                        },

                        gtk::Button {
                            set_label: "Add",
                            add_css_class: "suggested-action",
                            set_tooltip_text: Some("Add the tag"),
                            connect_clicked[sender, tag_input] => move |_| {
                                let tag = tag_input.text().as_str().trim().to_string();
                                if !tag.is_empty() {
                                    sender.input(FileDetailsInput::AddTag(tag));
                                    tag_input.set_text("");
                                }
                            },
                        },
                    },
                },

                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                },

                // Reading status section (moved up for better visibility)
                #[name(status_container)]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 12,
                },

                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                },

                // File details section
                #[name(file_details_container)]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 12,
                },

                // We'll add the StatusRadioGroup in the init method
            },
        }
    }

    async fn init(
        (file, file_data_source): Self::Init,
        root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        let (filename, folder) = ui_utils::extract_path_components(&file.path);

        let model = FileDetails {
            file,
            filename,
            folder,
            file_data_source,
            tag_container: None,
            tag_input: None,
            title_label: None,
            status_container: None,
            file_info_container: None,
            file_details_container: None,
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
        model.tag_input = Some(widgets.tag_input.clone());
        model.title_label = Some(widgets.title_label.clone());
        model.status_container = Some(widgets.status_container.clone());
        model.file_info_container = Some(widgets.file_info_container.clone());
        model.file_details_container = Some(widgets.file_details_container.clone());

        // Create the component instances
        let file_info_section = FileInfoSection::new(&model.file.type_, &model.filename, &model.folder);

        // Add the FileInfoSection to its container
        widgets.file_info_container.append(file_info_section.widget());

        // Add the FileDetailsSection to its container
        let file_details_section = FileDetailsSection::new(&model.file);
        widgets.file_details_container.append(file_details_section.widget());

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
                    tag_input.set_sensitive(false);
                    tag_input.set_progress_fraction(0.5);
                }

                // Try to add the tag
                let result = self
                    .file_data_source
                    .add_file_tags(self.file.id, vec![tag.clone()])
                    .await;

                // Reset the loading indicator
                if let Some(tag_input) = &self.tag_input {
                    tag_input.set_sensitive(true);
                    tag_input.set_progress_fraction(0.0);
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
                // Show a loading indicator in the tag input
                if let Some(tag_input) = &self.tag_input {
                    tag_input.set_sensitive(false);
                    tag_input.set_progress_fraction(0.5);
                }

                // Try to delete the tag
                let result = self
                    .file_data_source
                    .delete_file_tags(self.file.id, vec![tag.clone()])
                    .await;

                // Reset the loading indicator
                if let Some(tag_input) = &self.tag_input {
                    tag_input.set_sensitive(true);
                    tag_input.set_progress_fraction(0.0);
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
                // Focus the tag input field
                if let Some(tag_input) = &self.tag_input {
                    tag_input.grab_focus();
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
                    let file_info_section = FileInfoSection::new(&self.file.type_, &self.filename, &self.folder);
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
        }
    }
}
