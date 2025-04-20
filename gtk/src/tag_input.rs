use gtk::prelude::*;
use relm4::component::AsyncComponent;
use relm4::component::AsyncComponentParts;
use relm4::component::AsyncComponentSender;
use relm4::gtk;

/// Component for adding tags with an entry field, dropdown for existing tags, and add button
pub struct TagInput {
    /// Entry field for typing new tags
    tag_entry: Option<gtk::Entry>,
    /// Dropdown for selecting from existing tags
    tag_dropdown: Option<gtk::DropDown>,
    /// List of all available tags
    all_tags: Vec<String>,
    /// String list model for the dropdown
    tag_string_list: Option<gtk::StringList>,
    /// Placeholder text for the entry field
    placeholder_text: String,
    /// Label for the add button
    add_button_label: String,
}

#[derive(Debug)]
pub enum TagInputInput {
    /// Add a tag with the given name
    AddTag(String),
    /// Update the list of available tags
    UpdateTags(Vec<String>),
    /// Clear the entry field
    ClearEntry,
    /// Set the entry field to be loading (show progress indicator)
    SetLoading(bool),
}

#[derive(Debug)]
pub enum TagInputOutput {
    /// A tag was added
    TagAdded(String),
}

#[relm4::component(pub, async)]
impl AsyncComponent for TagInput {
    type Init = (Vec<String>, String, String);
    type Input = TagInputInput;
    type Output = TagInputOutput;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Horizontal,
            set_spacing: 8,
            set_margin_bottom: 8,
            add_css_class: "tag-input-box",
            add_css_class: "linked",
            set_margin_start: 4,
            set_margin_end: 4,

            #[name(tag_entry)]
            gtk::Entry {
                set_placeholder_text: Some(&model.placeholder_text),
                set_hexpand: true,
                set_tooltip_text: Some("Type a new tag"),
                set_accessible_role: gtk::AccessibleRole::SearchBox,
                add_css_class: "search-entry",
                connect_activate[sender] => move |entry| {
                    let tag = entry.text().as_str().trim().to_string();
                    if !tag.is_empty() {
                        sender.input(TagInputInput::AddTag(tag));
                    }
                },
            },

            #[name(tag_dropdown)]
            gtk::DropDown {
                set_tooltip_text: Some("Select from existing tags"),
                set_enable_search: true,
                set_show_arrow: true,
                add_css_class: "tag-dropdown",
                set_hexpand: false,
                set_width_request: 150,
                connect_selected_notify[sender, tag_entry] => move |dropdown| {
                    if dropdown.selected() != gtk::INVALID_LIST_POSITION && dropdown.selected() > 0 {
                        // Only process selection if it's not the placeholder (index 0)
                        if let Some(selected_item) = dropdown.selected_item() {
                            if let Some(string_object) = selected_item.downcast_ref::<gtk::StringObject>() {
                                let selected_tag = string_object.string();
                                // Don't set text if it's the placeholder
                                if selected_tag != "Select a tag..." {
                                    tag_entry.set_text(&selected_tag);
                                }
                            }
                        }
                        // Reset selection to the placeholder after using it
                        dropdown.set_selected(0);
                    }
                },
            },

            gtk::Button {
                set_label: &model.add_button_label,
                add_css_class: "suggested-action",
                set_tooltip_text: Some("Add the tag"),
                connect_clicked[sender, tag_entry] => move |_| {
                    let tag = tag_entry.text().as_str().trim().to_string();
                    if !tag.is_empty() {
                        sender.input(TagInputInput::AddTag(tag));
                    }
                },
            },
        }
    }

    async fn init(
        init: Self::Init,
        root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        let (all_tags, placeholder_text, add_button_label) = init;

        let model = TagInput {
            tag_entry: None,
            tag_dropdown: None,
            all_tags,
            tag_string_list: None,
            placeholder_text,
            add_button_label,
        };

        let widgets = view_output!();

        // Store references to widgets
        let mut model = model;
        model.tag_entry = Some(widgets.tag_entry.clone());
        model.tag_dropdown = Some(widgets.tag_dropdown.clone());

        // Set up tag dropdown with StringList model
        if let Some(dropdown) = &model.tag_dropdown {
            // Create a StringList for the dropdown with a placeholder
            let string_list = gtk::StringList::new(&["Select a tag..."]);
            dropdown.set_model(Some(&string_list));
            dropdown.set_selected(0); // Select the placeholder by default

            // Store the string list for later updates
            model.tag_string_list = Some(string_list);

            // Add available tags to the dropdown
            model.update_dropdown();
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
            TagInputInput::AddTag(tag) => {
                // Emit the tag added event
                sender
                    .output(TagInputOutput::TagAdded(tag.clone()))
                    .unwrap();

                // Clear the entry field
                if let Some(entry) = &self.tag_entry {
                    entry.set_text("");
                }
            }
            TagInputInput::UpdateTags(tags) => {
                self.all_tags = tags;
                self.update_dropdown();
            }
            TagInputInput::ClearEntry => {
                if let Some(entry) = &self.tag_entry {
                    entry.set_text("");
                }
            }
            TagInputInput::SetLoading(loading) => {
                if let Some(entry) = &self.tag_entry {
                    entry.set_sensitive(!loading);
                    entry.set_progress_fraction(if loading { 0.5 } else { 0.0 });
                }
                if let Some(dropdown) = &self.tag_dropdown {
                    dropdown.set_sensitive(!loading);
                }
            }
        }
    }
}

impl TagInput {
    /// Update the dropdown with the current list of tags
    fn update_dropdown(&self) {
        if let Some(string_list) = &self.tag_string_list {
            // Keep only the placeholder
            while string_list.n_items() > 1 {
                string_list.remove(1);
            }

            // Add all available tags
            for tag in &self.all_tags {
                string_list.append(tag);
            }
        }
    }
}
