use gtk::prelude::*;
use relm4::RelmWidgetExt;
use relm4::gtk;
use relm4::prelude::AsyncFactoryComponent;

use crate::ui_utils;
use archive_organizer::api::{File, ReadingStatus};

#[derive(Debug)]
pub struct FileBox {
    file: File,
    filename: String,
    folder: String,
    is_selected: bool,
}

impl FileBox {
    pub fn new(file: File) -> Self {
        let (filename, folder) = ui_utils::extract_path_components(&file.path);

        Self {
            file,
            filename,
            folder,
            is_selected: false,
        }
    }
}

#[derive(Debug)]
pub enum FileBoxInput {
    Clicked,
}

#[derive(Debug)]
pub enum FileBoxOutput {
    FileClicked(File),
}

#[relm4::factory(pub, async)]
impl AsyncFactoryComponent for FileBox {
    type Init = (File, Option<i32>);
    type Input = FileBoxInput;
    type Output = FileBoxOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;

    view! {
        #[root]
        gtk::Button {
            set_has_frame: false,
            set_can_focus: false,
            add_css_class: "file-item",
            set_css_classes: &[if self.is_selected { "selected-file" } else { "file-item" }],
            connect_clicked => FileBoxInput::Clicked,

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 12,
                set_margin_all: 8,

                // File icon
                gtk::Image {
                    set_icon_name: Some("text-x-generic-symbolic"),
                    add_css_class: "file-icon",
                    set_pixel_size: 24,
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 2,

                    #[name(filename)]
                    gtk::Label {
                        set_label: &self.filename,
                        set_hexpand: true,
                        set_halign: gtk::Align::Start,
                        add_css_class: "file-name",
                    },

                    #[name(folder)]
                    gtk::Label {
                        set_label: &self.folder,
                        set_hexpand: true,
                        set_halign: gtk::Align::Start,
                        add_css_class: "file-details",
                    },
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,
                    set_halign: gtk::Align::End,

                    // Reading status indicator
                    gtk::Label {
                        set_label: &format!("{:?}", self.file.status),
                        add_css_class: match self.file.status {
                            ReadingStatus::Unread => "dim-label",
                            ReadingStatus::Reading => "accent",
                            ReadingStatus::Read => "success",
                        },
                        set_margin_end: 8,
                    },

                    // Tags
                    #[name(tags)]
                    gtk::Label {
                        set_label: &self.file.tags.join(", "),
                        set_halign: gtk::Align::End,
                    },
                },
            }
        }
    }

    async fn init_model(
        init: Self::Init,
        _index: &relm4::prelude::DynamicIndex,
        _sender: relm4::AsyncFactorySender<Self>,
    ) -> Self {
        let (file, selected_id) = init;
        let mut model = Self::new(file);

        // Set selected state if this file matches the selected ID
        if let Some(id) = selected_id {
            model.is_selected = model.file.id == id;
        }

        model
    }

    async fn update(&mut self, message: Self::Input, sender: relm4::AsyncFactorySender<Self>) {
        match message {
            FileBoxInput::Clicked => {
                let _ = sender.output(FileBoxOutput::FileClicked(self.file.clone()));
            }
        }
    }
}
