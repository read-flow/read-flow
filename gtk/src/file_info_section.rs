use crate::ui_utils;
use gtk::prelude::*;
use relm4::gtk;

/// A component for displaying file information
pub struct FileInfoSection {
    container: gtk::Box,
}

impl FileInfoSection {
    /// Create a new file information section
    pub fn new(file_type: &str, filename: &str, folder: &str) -> Self {
        // Create the main container
        let container = gtk::Box::new(gtk::Orientation::Vertical, 12);
        container.set_margin_bottom(8);

        // Create the header with icon and type
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        header.set_spacing(12);
        header.set_margin_bottom(4);

        // Add the file type icon
        let icon = gtk::Image::new();
        icon.set_icon_name(Some(ui_utils::get_file_type_icon(file_type)));
        icon.set_pixel_size(36);
        icon.set_margin_end(8);
        header.append(&icon);

        // Add the file type information
        let type_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
        type_box.set_valign(gtk::Align::Center);

        let type_heading = gtk::Label::new(Some("File Information"));
        type_heading.add_css_class("caption-heading");
        type_heading.set_halign(gtk::Align::Start);
        type_box.append(&type_heading);

        let type_label = gtk::Label::new(Some(&format!("Type: {}", file_type.to_uppercase())));
        type_label.add_css_class("caption");
        type_label.add_css_class("dim-label");
        type_label.set_halign(gtk::Align::Start);
        type_box.append(&type_label);

        header.append(&type_box);
        container.append(&header);

        // Create the file details section
        let details = gtk::Box::new(gtk::Orientation::Vertical, 4);
        details.set_margin_start(8);
        details.set_margin_top(4);

        // Add filename row
        let filename_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let filename_label = gtk::Label::new(Some("Filename:"));
        filename_label.add_css_class("dim-label");
        filename_label.set_halign(gtk::Align::Start);
        filename_row.append(&filename_label);

        let filename_value = gtk::Label::new(Some(filename));
        filename_value.set_halign(gtk::Align::Start);
        filename_value.set_hexpand(true);
        filename_value.set_selectable(true);
        filename_value.set_ellipsize(gtk::pango::EllipsizeMode::End);
        filename_row.append(&filename_value);

        details.append(&filename_row);

        // Add location row
        let location_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let location_label = gtk::Label::new(Some("Location:"));
        location_label.add_css_class("dim-label");
        location_label.set_halign(gtk::Align::Start);
        location_row.append(&location_label);

        let location_value = gtk::Label::new(Some(folder));
        location_value.set_halign(gtk::Align::Start);
        location_value.set_hexpand(true);
        location_value.set_selectable(true);
        location_value.set_ellipsize(gtk::pango::EllipsizeMode::End);
        location_row.append(&location_value);

        details.append(&location_row);
        container.append(&details);

        Self { container }
    }

    /// Get the root widget
    pub fn widget(&self) -> &gtk::Box {
        &self.container
    }
}
