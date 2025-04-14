use std::path::Path;

use gtk::prelude::*;
use relm4::RelmWidgetExt;
use relm4::component::AsyncComponent;
use relm4::component::AsyncComponentParts;
use relm4::component::AsyncComponentSender;

use relm4::gtk;

use archive_organizer::api::File;
use archive_organizer::api::FileDataSource;

use crate::tag_badge::{TagBadge, TagBadgeHandler};

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
    status_label: Option<gtk::Label>,
}

#[derive(Debug)]
pub enum FileDetailsInput {
    Close,
    OpenFile,
    AddTag(String),
    DeleteTag(String),
    FocusTagInput,
    UpdateReadingStatus(archive_organizer::api::ReadingStatus),
}

#[derive(Debug)]
pub enum FileDetailsOutput {
    TagsChanged(i32),
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
        gtk::Window {
            set_title: Some("File Details"),
            set_default_width: 600,
            set_default_height: 600,
            set_modal: true,
            set_icon_name: Some("folder-archives"),
            add_css_class: "default-spacing",
            set_resizable: true,
            set_hide_on_close: false,

            // We'll handle responsive behavior with CSS media queries instead
            #[wrap(Some)]
            set_titlebar = &gtk::HeaderBar {
                set_show_title_buttons: true,
                #[wrap(Some)]
                set_title_widget = &gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 10,
                    set_valign: gtk::Align::Center,

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_valign: gtk::Align::Center,
                        set_hexpand: true,

                        gtk::Label {
                            set_label: &model.filename,
                            add_css_class: "title",
                            set_ellipsize: gtk::pango::EllipsizeMode::End,
                        },

                        gtk::Label {
                            set_label: &model.folder,
                            add_css_class: "subtitle",
                            set_ellipsize: gtk::pango::EllipsizeMode::End,
                        },
                    },
                },

                pack_end = &gtk::Button {
                    set_icon_name: "document-open-symbolic",
                    set_tooltip_text: Some("Open File (Ctrl+O)"),
                    add_css_class: "flat",
                    set_accessible_role: gtk::AccessibleRole::Button,
                    set_focusable: true,
                    set_focus_on_click: true,
                    connect_clicked[sender] => move |_| {
                        sender.input(FileDetailsInput::OpenFile);
                    },
                },
            },
            connect_close_request[sender] => move |_| {
                sender.input(FileDetailsInput::Close);
                gtk::glib::Propagation::Proceed
            },

            // We'll use key bindings in a different way

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 24,
                set_margin_all: 24,

                // File information section
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 12,
                    set_margin_bottom: 12,

                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 12,
                        set_margin_bottom: 8,

                        gtk::Image {
                            set_icon_name: Some(match model.file.type_.to_lowercase().as_str() {
                                "pdf" => "application-pdf-symbolic",
                                "epub" => "x-office-document-symbolic",
                                "mobi" => "ebook-reader-symbolic",
                                _ => "text-x-generic-symbolic",
                            }),
                            set_pixel_size: 48,
                            set_margin_end: 12,
                        },

                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 4,
                            set_valign: gtk::Align::Center,

                            gtk::Label {
                                set_label: "File Information",
                                add_css_class: "caption-heading",
                                set_halign: gtk::Align::Start,
                            },

                            gtk::Label {
                                set_label: &format!("Type: {}", model.file.type_.to_uppercase()),
                                add_css_class: "caption",
                                add_css_class: "dim-label",
                                set_halign: gtk::Align::Start,
                            },
                        },
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 4,

                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 8,
                            set_margin_start: 12,
                            set_margin_top: 8,

                            gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_spacing: 8,

                                gtk::Label {
                                    set_label: "Filename:",
                                    add_css_class: "dim-label",
                                    set_halign: gtk::Align::Start,
                                },

                                gtk::Label {
                                    set_label: &model.filename,
                                    set_halign: gtk::Align::Start,
                                    set_hexpand: true,
                                    set_selectable: true,
                                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                                },
                            },

                            gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_spacing: 8,

                                gtk::Label {
                                    set_label: "Location:",
                                    add_css_class: "dim-label",
                                    set_halign: gtk::Align::Start,
                                },

                                gtk::Label {
                                    set_label: &model.folder,
                                    set_halign: gtk::Align::Start,
                                    set_hexpand: true,
                                    set_selectable: true,
                                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                                },
                            },
                        },
                    },
                },

                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                },

                // Tags section
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 12,

                    gtk::Label {
                        set_label: "Tags",
                        add_css_class: "caption-heading",
                        set_halign: gtk::Align::Start,
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 12,
                        set_margin_bottom: 12,
                        add_css_class: "tag-input-box",
                        add_css_class: "linked",  // Add linked class for GNOME style

                        #[name(tag_input)]
                        gtk::Entry {
                            set_placeholder_text: Some("Add a new tag"),
                            set_hexpand: true,
                            set_tooltip_text: Some("Enter a tag and press Enter to add it (Ctrl+T)"),
                            set_accessible_role: gtk::AccessibleRole::SearchBox,
                            add_css_class: "search-entry",  // Add search-entry class for better styling
                            connect_activate[sender] => move |entry| {
                                let tag = entry.text().as_str().trim().to_string();
                                if !tag.is_empty() {
                                    sender.input(FileDetailsInput::AddTag(tag));
                                    entry.set_text("");
                                }
                            },

                            // Add a focus controller to properly handle focus events
                            add_controller = gtk::EventControllerFocus::new() {
                                connect_leave => move |_| {
                                    // Focus is leaving the entry - no action needed, just let GTK handle it
                                },
                            },
                        },

                        gtk::Button {
                            set_label: "Add",
                            add_css_class: "suggested-action",
                            set_tooltip_text: Some("Add the tag"),
                            set_accessible_role: gtk::AccessibleRole::Button,
                            set_focusable: true,
                            set_focus_on_click: true,
                            connect_clicked[sender, tag_input] => move |_| {
                                let tag = tag_input.text().as_str().trim().to_string();
                                if !tag.is_empty() {
                                    sender.input(FileDetailsInput::AddTag(tag));
                                    tag_input.set_text("");
                                }
                            },
                        },
                    },

                    gtk::Overlay {
                        #[name(tag_container)]
                        gtk::FlowBox {
                            set_selection_mode: gtk::SelectionMode::None,
                            set_max_children_per_line: 100,  // Allow many tags per line for better space usage
                            set_row_spacing: 2,  // Reduced spacing
                            set_column_spacing: 2,  // Reduced spacing
                            set_homogeneous: false,  // Don't make all children the same size
                            set_halign: gtk::Align::Fill,
                            set_hexpand: true,
                            set_vexpand: true,
                            set_margin_top: 4,  // Reduced margin
                            set_margin_bottom: 4,  // Reduced margin
                            set_margin_start: 4,  // Reduced margin
                            set_margin_end: 4,  // Reduced margin
                            set_visible: true,
                            add_css_class: "tag-container",  // Add the tag-container class
                        },
                    },
                },

                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                },

                // File details section
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 12,

                    gtk::Label {
                        set_label: "File Details",
                        add_css_class: "caption-heading",
                        set_halign: gtk::Align::Start,
                    },

                    gtk::ListBox {
                        add_css_class: "boxed-list",
                        add_css_class: "content-list",  // Add content-list class for GNOME style

                        // ID row
                        gtk::ListBoxRow {
                            gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_spacing: 12,
                                set_margin_all: 12,

                                gtk::Box {
                                    set_orientation: gtk::Orientation::Vertical,
                                    set_valign: gtk::Align::Center,
                                    set_hexpand: true,

                                    gtk::Label {
                                        set_label: "ID",
                                        add_css_class: "heading",
                                        set_halign: gtk::Align::Start,
                                    },

                                    gtk::Label {
                                        set_label: &model.file.id.to_string(),
                                        set_selectable: true,
                                        set_halign: gtk::Align::Start,
                                    },
                                },
                            },
                        },

                        // Type row
                        gtk::ListBoxRow {
                            gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_spacing: 12,
                                set_margin_all: 12,

                                gtk::Box {
                                    set_orientation: gtk::Orientation::Vertical,
                                    set_valign: gtk::Align::Center,
                                    set_hexpand: true,

                                    gtk::Label {
                                        set_label: "Type",
                                        add_css_class: "heading",
                                        set_halign: gtk::Align::Start,
                                    },

                                    gtk::Label {
                                        set_label: &model.file.type_,
                                        set_selectable: true,
                                        set_halign: gtk::Align::Start,
                                    },
                                },
                            },
                        },

                        // Size row
                        gtk::ListBoxRow {
                            gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_spacing: 12,
                                set_margin_all: 12,

                                gtk::Box {
                                    set_orientation: gtk::Orientation::Vertical,
                                    set_valign: gtk::Align::Center,
                                    set_hexpand: true,

                                    gtk::Label {
                                        set_label: "Size",
                                        add_css_class: "heading",
                                        set_halign: gtk::Align::Start,
                                    },

                                    gtk::Label {
                                        set_label: &format!("{} bytes", model.file.size),
                                        set_selectable: true,
                                        set_halign: gtk::Align::Start,
                                    },
                                },
                            },
                        },

                        // Fingerprint row
                        gtk::ListBoxRow {
                            gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_spacing: 12,
                                set_margin_all: 12,

                                gtk::Box {
                                    set_orientation: gtk::Orientation::Vertical,
                                    set_valign: gtk::Align::Center,
                                    set_hexpand: true,

                                    gtk::Label {
                                        set_label: "Fingerprint",
                                        add_css_class: "heading",
                                        set_halign: gtk::Align::Start,
                                    },

                                    gtk::Label {
                                        set_label: &model.file.fingerprint,
                                        set_selectable: true,
                                        set_halign: gtk::Align::Start,
                                        set_wrap: true,
                                        set_wrap_mode: gtk::pango::WrapMode::WordChar,
                                    },
                                },
                            },
                        },

                        // Status row
                        gtk::ListBoxRow {
                            gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_spacing: 12,
                                set_margin_all: 12,

                                gtk::Box {
                                    set_orientation: gtk::Orientation::Vertical,
                                    set_valign: gtk::Align::Center,
                                    set_hexpand: true,

                                    gtk::Label {
                                        set_label: "Reading Status",
                                        add_css_class: "heading",
                                        set_halign: gtk::Align::Start,
                                    },

                                    gtk::Box {
                                        set_orientation: gtk::Orientation::Horizontal,
                                        set_spacing: 12,
                                        set_margin_top: 8,
                                        set_margin_bottom: 4,

                                        // Radio button for Unread status
                                        gtk::Box {
                                            set_orientation: gtk::Orientation::Horizontal,
                                            set_spacing: 4,

                                            #[name(unread_radio)]
                                            gtk::CheckButton {
                                                set_active: model.file.status == archive_organizer::api::ReadingStatus::Unread,
                                                add_css_class: "radio",
                                                connect_toggled[sender] => move |btn| {
                                                    if btn.is_active() {
                                                        sender.input(FileDetailsInput::UpdateReadingStatus(archive_organizer::api::ReadingStatus::Unread));
                                                    }
                                                },
                                            },

                                            gtk::Label {
                                                set_label: "Unread",
                                                set_margin_start: 4,
                                            },
                                        },

                                        // Radio button for Reading status
                                        gtk::Box {
                                            set_orientation: gtk::Orientation::Horizontal,
                                            set_spacing: 4,
                                            set_margin_start: 12,

                                            #[name(reading_radio)]
                                            gtk::CheckButton {
                                                set_active: model.file.status == archive_organizer::api::ReadingStatus::Reading,
                                                set_group: Some(&unread_radio),
                                                add_css_class: "radio",
                                                connect_toggled[sender] => move |btn| {
                                                    if btn.is_active() {
                                                        sender.input(FileDetailsInput::UpdateReadingStatus(archive_organizer::api::ReadingStatus::Reading));
                                                    }
                                                },
                                            },

                                            gtk::Label {
                                                set_label: "Reading",
                                                set_margin_start: 4,
                                            },
                                        },

                                        // Radio button for Read status
                                        gtk::Box {
                                            set_orientation: gtk::Orientation::Horizontal,
                                            set_spacing: 4,
                                            set_margin_start: 12,

                                            gtk::CheckButton {
                                                set_active: model.file.status == archive_organizer::api::ReadingStatus::Read,
                                                set_group: Some(&unread_radio),
                                                add_css_class: "radio",
                                                connect_toggled[sender] => move |btn| {
                                                    if btn.is_active() {
                                                        sender.input(FileDetailsInput::UpdateReadingStatus(archive_organizer::api::ReadingStatus::Read));
                                                    }
                                                },
                                            },

                                            gtk::Label {
                                                set_label: "Read",
                                                set_margin_start: 4,
                                            },
                                        },
                                    },

                                    #[name(status_label)]
                                    gtk::Label {
                                        set_label: &format!("Current status: {:?}", model.file.status),
                                        set_margin_top: 4,
                                        add_css_class: "caption",
                                        add_css_class: "dim-label",
                                        set_halign: gtk::Align::Start,
                                    },
                                },
                            },
                        },
                    },
                },
            },
        }
    }

    async fn init(
        (file, file_data_source): Self::Init,
        root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        let filename = Path::new(&file.path)
            .file_name()
            .and_then(|os_str| os_str.to_str())
            .unwrap_or("Unknown file")
            .to_string();

        let folder = Path::new(&file.path)
            .parent()
            .and_then(|path| path.to_str())
            .unwrap_or("Unknown folder")
            .to_string();

        let model = FileDetails {
            file,
            filename,
            folder,
            file_data_source,
            tag_container: None,
            tag_input: None,
            status_label: None,
        };

        let widgets = view_output!();

        // Add tag badges
        // Create a tag handler with the sender
        let tag_handler = FileDetailsTagHandler {
            sender: sender.input_sender().clone(),
        };

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

        root.present();

        // Store references to widgets in the model
        let mut model = model;
        model.tag_container = Some(widgets.tag_container.clone());
        model.tag_input = Some(widgets.tag_input.clone());
        model.status_label = Some(widgets.status_label.clone());

        AsyncComponentParts { model, widgets }
    }

    async fn update(
        &mut self,
        msg: Self::Input,
        sender: AsyncComponentSender<Self>,
        root: &Self::Root,
    ) {
        match msg {
            FileDetailsInput::Close => {
                // Notify that we're closing in case any tags were changed
                sender
                    .output(FileDetailsOutput::TagsChanged(self.file.id))
                    .unwrap();
                root.close();
            }
            FileDetailsInput::OpenFile => {
                if let Err(e) = self.file_data_source.xdg_open_file(self.file.clone()).await {
                    tracing::warn!("Error opening file: {}", e);
                }
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

                    // Update the status label
                    if let Some(status_label) = &self.status_label {
                        status_label.set_label(&format!("Current status: {:?}", self.file.status));
                    }

                    // Update the file in the database
                    let result = self.file_data_source.update_file(self.file.clone()).await;

                    if let Err(e) = result {
                        tracing::warn!("Failed to update reading status: {}", e);
                    } else {
                        // Notify that the file has been updated
                        sender
                            .output(FileDetailsOutput::TagsChanged(self.file.id))
                            .unwrap();
                    }
                }
            }
        }
    }
}
