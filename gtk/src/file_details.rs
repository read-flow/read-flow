use std::path::Path;
use std::sync::Arc;

use gtk::prelude::*;
use relm4::component::AsyncComponent;
use relm4::component::AsyncComponentParts;
use relm4::component::AsyncComponentSender;
use relm4::gtk;

use archive_organizer::api::File;
use archive_organizer::api::FileDataSource;

struct TagBadge {
    label: gtk::Label,
}

impl TagBadge {
    fn new(tag: &str) -> Self {
        let label = gtk::Label::new(Some(tag));
        label.add_css_class("tag-badge");
        label.set_selectable(true);
        Self { label }
    }
}

pub struct FileDetails<FDS> {
    file: File,
    filename: String,
    folder: String,
    file_data_source: Arc<FDS>,
}

#[derive(Debug)]
pub enum FileDetailsInput {
    Close,
    OpenFile,
}

#[relm4::component(pub, async)]
impl<FDS> AsyncComponent for FileDetails<FDS>
where
    FDS: FileDataSource + 'static,
{
    type Init = (File, Arc<FDS>);
    type Input = FileDetailsInput;
    type Output = ();
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Window {
            set_title: Some("File Details"),
            set_default_width: 500,
            set_default_height: 600,
            set_modal: true,
            add_css_class: "about-dialog",
            connect_close_request[sender] => move |_| {
                sender.input(FileDetailsInput::Close);
		gtk::glib::Propagation::Proceed
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 12,
                    set_margin_start: 20,
                    set_margin_end: 20,
                    set_margin_top: 20,
                    set_margin_bottom: 20,

                    #[name(filename_label)]
                    gtk::Label {
                        set_label: &model.filename,
                        add_css_class: "title-1",
                        set_halign: gtk::Align::Center,
                    },

                    #[name(folder_label)]
                    gtk::Label {
                        set_label: &model.folder,
                        add_css_class: "dim-label",
                        set_halign: gtk::Align::Center,
                        set_selectable: true,
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 8,
                        set_margin_top: 12,

                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 8,
                            set_halign: gtk::Align::End,

                            #[name(tag_container)]
                            gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_spacing: 8,
                            },
                        },
                    },
                },

                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                },

                gtk::ScrolledWindow {
                    set_vexpand: true,
                    set_hexpand: true,
                    set_min_content_height: 300,

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 12,
                        set_margin_start: 20,
                        set_margin_end: 20,
                        set_margin_top: 20,
                        set_margin_bottom: 20,

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

                gtk::Separator {
                    set_orientation: gtk::Orientation::Horizontal,
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 12,
                    set_margin_start: 20,
                    set_margin_end: 20,
                    set_margin_top: 20,

                    gtk::Button {
                        set_label: "Open File",
                        set_hexpand: true,
                        connect_clicked[sender] => move |_| {
                            sender.input(FileDetailsInput::OpenFile);
                        },
                    },
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 12,
                    set_margin_start: 20,
                    set_margin_end: 20,
                    set_margin_top: 12,
                    set_margin_bottom: 12,

                    gtk::Button {
                        set_label: "Close",
                        connect_clicked => FileDetailsInput::Close,
                    },
                },
            }
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
        };

        let widgets = view_output!();

        // Add tag badges
        for tag in &model.file.tags {
            let badge = TagBadge::new(tag);
            widgets.tag_container.append(&badge.label);
        }

        root.present();

        AsyncComponentParts { model, widgets }
    }

    async fn update(&mut self, msg: Self::Input, _sender: AsyncComponentSender<Self>, root: &Self::Root) {
        match msg {
            FileDetailsInput::Close => {
                root.close();
            }
            FileDetailsInput::OpenFile => {
                if let Err(e) = self.file_data_source.xdg_open_file(self.file.clone()).await {
                    eprintln!("Error opening file: {}", e);
                }
            }
        }
    }
}
