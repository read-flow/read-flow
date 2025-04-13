use std::path::Path;

use gtk::prelude::*;
use relm4::gtk;
use relm4::prelude::AsyncFactoryComponent;

use archive_organizer::api::{File, ReadingStatus};

#[derive(Debug)]
pub struct FileBox {
    file: File,
    filename: String,
    folder: String,
}

impl FileBox {
    pub fn new(file: File) -> Self {
        let path: &Path = file.path.as_ref();
        let folder = format!("{}", path.parent().unwrap().display());
        let filename = format!("{}", path.file_name().unwrap().to_string_lossy());

        Self {
            file,
            filename,
            folder,
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
    type Init = File;
    type Input = FileBoxInput;
    type Output = FileBoxOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;

    view! {
        #[root]
        gtk::Button {
            set_has_frame: false,
            set_can_focus: false,
            connect_clicked => FileBoxInput::Clicked,

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 10,

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 0,

                    #[name(filename)]
                    gtk::Label {
                        set_label: &self.filename,
                        set_hexpand: true,
                        set_halign: gtk::Align::Start,
                    },

                    #[name(folder)]
                    gtk::Label {
                        set_label: &self.folder,
                        set_hexpand: true,
                        set_halign: gtk::Align::Start,
                        add_css_class: "my-path",
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
        file: Self::Init,
        _index: &relm4::prelude::DynamicIndex,
        _sender: relm4::AsyncFactorySender<Self>,
    ) -> Self {
        Self::new(file)
    }

    async fn update(&mut self, message: Self::Input, sender: relm4::AsyncFactorySender<Self>) {
        match message {
            FileBoxInput::Clicked => {
                let _ = sender.output(FileBoxOutput::FileClicked(self.file.clone()));
            }
        }
    }
}
