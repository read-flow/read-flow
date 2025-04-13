use gtk::prelude::*;
use relm4::gtk;

/// A trait for handling tag badge actions
pub trait TagBadgeHandler {
    /// Called when the delete button is clicked
    fn on_delete_tag(&self, tag: String);
}

/// A reusable tag badge component
pub struct TagBadge {
    container: gtk::Box,
    label: gtk::Label,
}

impl TagBadge {
    /// Create a new tag badge
    pub fn new<H>(tag: &str, handler: &H) -> Self
    where
        H: TagBadgeHandler + Clone + 'static,
    {
        // Create a container with horizontal orientation
        let container = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        container.add_css_class("card");
        container.add_css_class("tag-badge");
        container.set_margin_end(4);
        container.set_margin_bottom(4);

        // Add a small icon to represent the tag
        let icon = gtk::Image::new();
        icon.set_icon_name(Some("tag-symbolic"));
        icon.set_pixel_size(12);
        icon.set_margin_start(6);
        icon.set_margin_top(4);
        icon.set_margin_bottom(4);
        icon.set_opacity(0.7);

        // Create the label for the tag text
        let label = gtk::Label::new(Some(tag));
        label.add_css_class("caption");
        label.set_margin_start(4);
        label.set_margin_end(2);
        label.set_margin_top(4);
        label.set_margin_bottom(4);

        // Create the delete button
        let delete_button = gtk::Button::new();
        delete_button.set_icon_name("window-close-symbolic");
        delete_button.add_css_class("flat");
        delete_button.add_css_class("circular");
        delete_button.set_valign(gtk::Align::Center);
        delete_button.set_tooltip_text(Some("Remove tag"));

        // Connect the delete button to the handler
        let tag_clone = tag.to_string();
        let handler_clone = handler.clone();
        delete_button.connect_clicked(move |_| {
            handler_clone.on_delete_tag(tag_clone.clone());
        });

        // Add all elements to the container
        container.append(&icon);
        container.append(&label);
        container.append(&delete_button);

        Self { container, label }
    }

    /// Get the root widget of the tag badge
    pub fn widget(&self) -> &gtk::Box {
        &self.container
    }

    /// Get the tag text
    pub fn get_tag(&self) -> String {
        self.label.text().to_string()
    }
}
