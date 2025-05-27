use crate::ui_utils;
use archive_organizer::api::File;
use gtk::prelude::*;
use relm4::gtk;

/// A component for displaying detailed file properties
pub struct FileDetailsSection {
    container: gtk::Box,
}

impl FileDetailsSection {
    /// Create a new file details section
    pub fn new(file: &File) -> Self {
        // Create the main container
        let container = gtk::Box::new(gtk::Orientation::Vertical, 12);
        container.add_css_class("details-panel-section");

        // Create the heading
        let heading = ui_utils::create_heading_label("File Details");
        heading.add_css_class("details-panel-section-title");
        container.append(&heading);

        // Create the list box for details
        let list_box = gtk::ListBox::new();
        list_box.add_css_class("boxed-list");
        list_box.add_css_class("content-list");

        // Add ID row
        let id_row = ui_utils::create_detail_row("ID", &file.id.to_string());
        list_box.append(&id_row);

        // Add Type row
        let type_row = ui_utils::create_detail_row("Type", &file.type_);
        list_box.append(&type_row);

        // Add Size row
        let size_row = ui_utils::create_detail_row("Size", &format!("{} bytes", file.size));
        list_box.append(&size_row);

        // Add Fingerprint row
        let fingerprint_row = ui_utils::create_detail_row("Fingerprint", &file.fingerprint);
        list_box.append(&fingerprint_row);

        container.append(&list_box);

        Self { container }
    }

    /// Get the root widget
    pub fn widget(&self) -> &gtk::Box {
        &self.container
    }
}
