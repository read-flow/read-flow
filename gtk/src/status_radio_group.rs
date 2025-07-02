use archive_organizer::api::ReadingStatus;
use gtk::prelude::*;
use relm4::RelmWidgetExt;
use relm4::gtk;

/// A component for displaying and selecting reading status
pub struct StatusRadioGroup {
    container: gtk::Box,
    unread_radio: gtk::CheckButton,
    reading_radio: gtk::CheckButton,
    read_radio: gtk::CheckButton,
    status_label: gtk::Label,
}

impl StatusRadioGroup {
    /// Create a new status radio group
    pub fn new<F>(current_status: ReadingStatus, on_status_change: F) -> Self
    where
        F: Fn(ReadingStatus) + Clone + 'static,
    {
        // Create the main container
        let container = gtk::Box::new(gtk::Orientation::Vertical, 8);
        container.set_margin_all(8);

        // Create the heading
        let heading = gtk::Label::new(Some("Reading Status"));
        heading.add_css_class("heading");
        heading.set_halign(gtk::Align::Start);
        container.append(&heading);

        // Create the radio button container
        let radio_container = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        radio_container.set_margin_top(6);
        radio_container.set_margin_bottom(2);

        // Create the radio buttons

        // Unread radio button
        let unread_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let unread_radio = gtk::CheckButton::new();
        unread_radio.set_active(current_status == ReadingStatus::Unread);
        unread_radio.add_css_class("radio");

        let unread_label = gtk::Label::new(Some("Unread"));
        unread_label.set_margin_start(4);

        unread_box.append(&unread_radio);
        unread_box.append(&unread_label);
        radio_container.append(&unread_box);

        // Reading radio button
        let reading_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        reading_box.set_margin_start(8);

        let reading_radio = gtk::CheckButton::new();
        reading_radio.set_active(current_status == ReadingStatus::Reading);
        reading_radio.set_group(Some(&unread_radio));
        reading_radio.add_css_class("radio");

        let reading_label = gtk::Label::new(Some("Reading"));
        reading_label.set_margin_start(4);

        reading_box.append(&reading_radio);
        reading_box.append(&reading_label);
        radio_container.append(&reading_box);

        // Read radio button
        let read_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        read_box.set_margin_start(8);

        let read_radio = gtk::CheckButton::new();
        read_radio.set_active(current_status == ReadingStatus::Read);
        read_radio.set_group(Some(&unread_radio));
        read_radio.add_css_class("radio");

        let read_label = gtk::Label::new(Some("Read"));
        read_label.set_margin_start(4);

        read_box.append(&read_radio);
        read_box.append(&read_label);
        radio_container.append(&read_box);

        container.append(&radio_container);

        // Create the status label
        let status_label = gtk::Label::new(Some(&format!("Current status: {current_status:?}")));
        status_label.set_margin_top(4);
        status_label.add_css_class("caption");
        status_label.add_css_class("dim-label");
        status_label.set_halign(gtk::Align::Start);
        container.append(&status_label);

        // Connect signals
        let on_status_change_unread = on_status_change.clone();
        unread_radio.connect_toggled(move |btn| {
            if btn.is_active() {
                on_status_change_unread(ReadingStatus::Unread);
            }
        });

        let on_status_change_reading = on_status_change.clone();
        reading_radio.connect_toggled(move |btn| {
            if btn.is_active() {
                on_status_change_reading(ReadingStatus::Reading);
            }
        });

        let on_status_change_read = on_status_change.clone();
        read_radio.connect_toggled(move |btn| {
            if btn.is_active() {
                on_status_change_read(ReadingStatus::Read);
            }
        });

        Self {
            container,
            unread_radio,
            reading_radio,
            read_radio,
            status_label,
        }
    }

    /// Get the root widget
    pub fn widget(&self) -> &gtk::Box {
        &self.container
    }

    /// Update the current status
    pub fn set_status(&self, status: ReadingStatus) {
        match status {
            ReadingStatus::Unread => self.unread_radio.set_active(true),
            ReadingStatus::Reading => self.reading_radio.set_active(true),
            ReadingStatus::Read => self.read_radio.set_active(true),
        }
        self.status_label
            .set_label(&format!("Current status: {status:?}"));
    }

    /// Get the current status
    pub fn get_status(&self) -> ReadingStatus {
        if self.unread_radio.is_active() {
            ReadingStatus::Unread
        } else if self.reading_radio.is_active() {
            ReadingStatus::Reading
        } else {
            ReadingStatus::Read
        }
    }
}
