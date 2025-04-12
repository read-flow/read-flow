use std::path::Path;

use gtk::prelude::*;
use relm4::RelmRemoveAllExt;
use relm4::RelmWidgetExt;
use relm4::component::AsyncComponent;
use relm4::component::AsyncComponentParts;
use relm4::component::AsyncComponentSender;

use relm4::gtk;

use archive_organizer::api::File;
use archive_organizer::api::FileDataSource;

struct TagBadge {
    container: gtk::Box,
    label: gtk::Label,
    delete_button: gtk::Button,
}

impl TagBadge {
    fn new<S>(
        tag: &str,
        sender: &S,
    ) -> Self
    where
        S: Fn(FileDetailsInput) + Clone + 'static,
    {
        let container = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        container.add_css_class("tag-badge-container");

        let label = gtk::Label::new(Some(tag));
        label.add_css_class("tag-badge");
        label.set_selectable(true);

        let delete_button = gtk::Button::new();
        delete_button.set_label("×");
        delete_button.add_css_class("tag-delete-button");

        let tag_clone = tag.to_string();
        let sender_clone = sender.clone();
        delete_button.connect_clicked(move |_| {
            sender_clone(FileDetailsInput::DeleteTag(tag_clone.clone()));
        });

        container.append(&label);
        container.append(&delete_button);

        Self {
            container,
            label,
            delete_button,
        }
    }
}

pub struct FileDetails<FDS> {
    file: File,
    filename: String,
    folder: String,
    file_data_source: FDS,
    tag_container: Option<gtk::Box>,
}

#[derive(Debug)]
pub enum FileDetailsInput {
    Close,
    OpenFile,
    AddTag(String),
    DeleteTag(String),
}

#[derive(Debug)]
pub enum FileDetailsOutput {
    TagsChanged(i32),
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
            add_css_class: "about-dialog",
            connect_close_request[sender] => move |_| {
                sender.input(FileDetailsInput::Close);
                gtk::glib::Propagation::Proceed
            },

            gtk::HeaderBar {
                set_show_title_buttons: true,
                #[wrap(Some)]
                set_title_widget = &gtk::Label::new(Some("File Details")),
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 12,
                set_margin_all: 12,

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 12,

                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 8,
                        set_halign: gtk::Align::End,
                        set_hexpand: true,

                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 8,
                            set_halign: gtk::Align::End,

                            #[name(tag_input)]
                            gtk::Entry {
                                set_placeholder_text: Some("Enter new tag"),
                                set_width_request: 200,
                                connect_activate[sender] => move |entry| {
                                    let tag = entry.text().as_str().trim().to_string();
                                    if !tag.is_empty() {
                                        sender.input(FileDetailsInput::AddTag(tag));
                                        entry.set_text("");
                                    }
                                },
                            },

                            #[name(tag_container)]
                            gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_spacing: 8,
                                set_halign: gtk::Align::End,
                            },
                        },
                    },

                    #[name(filename_label)]
                    gtk::Label {
                        set_label: &model.filename,
                        add_css_class: "title-1",
                        set_halign: gtk::Align::Start,
                    },

                    #[name(folder_label)]
                    gtk::Label {
                        set_label: &model.folder,
                        add_css_class: "dim-label",
                        set_halign: gtk::Align::Start,
                        set_selectable: true,
                    },

                    gtk::ScrolledWindow {
                        set_vexpand: true,
                        set_hexpand: true,
                        set_min_content_height: 300,
                        set_policy: (gtk::PolicyType::Automatic, gtk::PolicyType::Automatic),

                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 12,
                            set_margin_all: 12,

                            #[name(details_label)]
                            gtk::Label {
                                set_label: "Details",
                                add_css_class: "title-2",
                                set_halign: gtk::Align::Start,
                            },

                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_spacing: 12,

                                gtk::Box {
                                    set_orientation: gtk::Orientation::Horizontal,
                                    set_spacing: 12,

                                    #[name(id_label)]
                                    gtk::Label {
                                        set_label: "ID",
                                        add_css_class: "dim-label",
                                        set_width_request: 100,
                                    },
                                    #[name(id_value)]
                                    gtk::Label {
                                        set_label: &model.file.id.to_string(),
                                        set_selectable: true,
                                    },
                                },

                                gtk::Box {
                                    set_orientation: gtk::Orientation::Horizontal,
                                    set_spacing: 12,

                                    #[name(type_label)]
                                    gtk::Label {
                                        set_label: "Type",
                                        add_css_class: "dim-label",
                                        set_width_request: 100,
                                    },
                                    #[name(type_value)]
                                    gtk::Label {
                                        set_label: &model.file.type_,
                                        set_selectable: true,
                                    },
                                },

                                gtk::Box {
                                    set_orientation: gtk::Orientation::Horizontal,
                                    set_spacing: 12,

                                    #[name(size_label)]
                                    gtk::Label {
                                        set_label: "Size",
                                        add_css_class: "dim-label",
                                        set_width_request: 100,
                                    },
                                    #[name(size_value)]
                                    gtk::Label {
                                        set_label: &format!("{} bytes", model.file.size),
                                        set_selectable: true,
                                    },
                                },

                                gtk::Box {
                                    set_orientation: gtk::Orientation::Horizontal,
                                    set_spacing: 12,

                                    #[name(fingerprint_label)]
                                    gtk::Label {
                                        set_label: "Fingerprint",
                                        add_css_class: "dim-label",
                                        set_width_request: 100,
                                    },
                                    #[name(fingerprint_value)]
                                    gtk::Label {
                                        set_label: &model.file.fingerprint,
                                        set_selectable: true,
                                    },
                                },

                                gtk::Box {
                                    set_orientation: gtk::Orientation::Horizontal,
                                    set_spacing: 12,

                                    #[name(status_label)]
                                    gtk::Label {
                                        set_label: "Status",
                                        add_css_class: "dim-label",
                                        set_width_request: 100,
                                    },
                                    #[name(status_value)]
                                    gtk::Label {
                                        set_label: &format!("{:?}", model.file.status),
                                        set_selectable: true,
                                    },
                                },
                            },
                        },
                    },
                },

                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 12,
                    set_margin_all: 12,
                    set_halign: gtk::Align::End,

                    gtk::Button {
                        set_label: "Open File",
                        add_css_class: "suggested-action",
                        connect_clicked[sender] => move |_| {
                            sender.input(FileDetailsInput::OpenFile);
                        },
                    },

                    gtk::Button {
                        set_label: "Close",
                        add_css_class: "destructive-action",
                        connect_clicked => FileDetailsInput::Close,
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
        };

        let widgets = view_output!();

        // Add tag badges
        // Create a sender clone outside the loop
        let sender_clone = sender.clone();
        for tag in &model.file.tags {
            let sender_clone = sender_clone.clone();
            let badge = TagBadge::new(tag, &move |input| {
                sender_clone.input(input);
            });
            widgets.tag_container.append(&badge.container);
        }

        root.present();

        // Store a reference to the tag_container in the model
        let mut model = model;
        model.tag_container = Some(widgets.tag_container.clone());

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
                    eprintln!("Error opening file: {}", e);
                }
            }
            FileDetailsInput::AddTag(tag) => {
                if let Err(e) = self
                    .file_data_source
                    .add_file_tags(self.file.id, vec![tag.clone()])
                    .await
                {
                    eprintln!("Error adding tag: {}", e);
                } else {
                    // Refresh the tags display
                    if let Ok(updated_file) = self.file_data_source.get_file(self.file.id).await {
                        // unwrap is safe, because otherwise the `add_file_tags` would fail.
                        self.file = updated_file.unwrap();
                        // Clear existing tags
                        if let Some(tag_container) = &self.tag_container {
                            tag_container.remove_all();
                            // Add new tags
                            // Create a sender clone outside the loop
                            let sender_clone = sender.clone();
                            for tag in &self.file.tags {
                                let sender_clone = sender_clone.clone();
                                let badge = TagBadge::new(tag, &move |input| {
                                    sender_clone.input(input);
                                });
                                tag_container.append(&badge.container);
                            }
                        }
                        // Notify that tags have changed
                        sender
                            .output(FileDetailsOutput::TagsChanged(self.file.id))
                            .unwrap();
                    }
                }
            }
            FileDetailsInput::DeleteTag(tag) => {
                if let Err(e) = self
                    .file_data_source
                    .delete_file_tags(self.file.id, vec![tag.clone()])
                    .await
                {
                    eprintln!("Error deleting tag: {}", e);
                } else {
                    // Refresh the tags display
                    if let Ok(updated_file) = self.file_data_source.get_file(self.file.id).await {
                        // unwrap is safe, because otherwise the `delete_file_tags` would fail.
                        self.file = updated_file.unwrap();
                        // Clear existing tags
                        if let Some(tag_container) = &self.tag_container {
                            tag_container.remove_all();
                            // Add new tags
                            // Create a sender clone outside the loop
                            let sender_clone = sender.clone();
                            for tag in &self.file.tags {
                                let sender_clone = sender_clone.clone();
                                let badge = TagBadge::new(tag, &move |input| {
                                    sender_clone.input(input);
                                });
                                tag_container.append(&badge.container);
                            }
                        }
                        // Notify that tags have changed
                        sender
                            .output(FileDetailsOutput::TagsChanged(self.file.id))
                            .unwrap();
                    }
                }
            }
        }
    }
}
