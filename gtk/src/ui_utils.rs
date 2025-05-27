use std::path::Path;

use gtk::prelude::*;
use relm4::RelmWidgetExt;
use relm4::gtk;

/// Extracts filename and folder from a file path
pub fn extract_path_components(path: &str) -> (String, String) {
    let path = Path::new(path);
    let filename = path
        .file_name()
        .and_then(|os_str| os_str.to_str())
        .unwrap_or("Unknown file")
        .to_string();

    let folder = path
        .parent()
        .and_then(|path| path.to_str())
        .unwrap_or("Unknown folder")
        .to_string();

    (filename, folder)
}

/// Creates a label with specific styling
pub fn create_heading_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("caption-heading");
    label.set_halign(gtk::Align::Start);
    label
}

/// Creates a detail row with a label and value
pub fn create_detail_row(label_text: &str, value_text: &str) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.add_css_class("details-property-row");

    let box_container = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    box_container.set_margin_all(8);

    let content_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    content_box.set_valign(gtk::Align::Center);
    content_box.set_hexpand(true);

    let label = gtk::Label::new(Some(label_text));
    label.add_css_class("heading");
    label.add_css_class("details-property-label");
    label.set_halign(gtk::Align::Start);

    let value = gtk::Label::new(Some(value_text));
    value.set_selectable(true);
    value.set_halign(gtk::Align::Start);
    value.add_css_class("details-property-value");

    if value_text.len() > 50 {
        value.set_wrap(true);
        value.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    }

    content_box.append(&label);
    content_box.append(&value);
    box_container.append(&content_box);
    row.set_child(Some(&box_container));

    row
}

/// Creates a section container with standard styling
pub fn create_section_container() -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Vertical, 12)
}

/// Returns the appropriate icon name for a file type
pub fn get_file_type_icon(file_type: &str) -> &'static str {
    match file_type.to_lowercase().as_str() {
        "pdf" => "application-pdf-symbolic",
        "epub" => "x-office-document-symbolic",
        "mobi" => "ebook-reader-symbolic",
        _ => "text-x-generic-symbolic",
    }
}
