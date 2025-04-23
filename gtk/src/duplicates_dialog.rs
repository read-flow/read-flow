use relm4::prelude::*;
use relm4::gtk::prelude::*;
use std::sync::Arc;

use archive_organizer::api::{File, FileDataSource};

// We don't need the clone_box function anymore

#[derive(Debug)]
pub enum DuplicatesDialogInput {
    DeleteFile(File),
    ConfirmDeleteFile(File),
}

#[derive(Debug)]
pub enum DuplicatesDialogOutput {
    FileDeleted,
}

pub struct DuplicatesDialogInit {
    pub duplicates: Vec<Vec<File>>,
    pub file_data_source: FDS,
}

// We need to use a type parameter for the FileDataSource implementation
pub type FDS = Arc<dyn FileDataSource<Error = Box<dyn std::error::Error + Send + Sync>> + Send + Sync>;

pub struct DuplicatesDialog {
    duplicates: Vec<Vec<File>>,
    file_data_source: FDS,
}

#[relm4::component(pub, async)]
impl AsyncComponent for DuplicatesDialog
{
    type Init = DuplicatesDialogInit;
    type Input = DuplicatesDialogInput;
    type Output = DuplicatesDialogOutput;
    type CommandOutput = ();

    view! {
        dialog = gtk::Dialog {
            set_title: Some("Duplicate Files"),
            set_default_width: 600,
            set_default_height: 400,
            add_button: ("Close", gtk::ResponseType::Close),

            connect_response[sender] => move |dialog, _| {
                dialog.close();
            },

            #[wrap(Some)]
            set_child = &gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 12,
                set_margin_all: 16,

                // Add a label with instructions
                append = &gtk::Label {
                    set_label: &format!("{} groups of duplicate files found", model.duplicates.len()),
                    add_css_class: "title-3",
                    set_halign: gtk::Align::Start,
                },

                append = &gtk::Label {
                    set_label: "Select files to delete from each group of duplicates.",
                    set_halign: gtk::Align::Start,
                    set_margin_bottom: 12,
                },

                // Create a scrolled window for the duplicate groups
                append = &gtk::ScrolledWindow {
                    set_vexpand: true,
                    set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),

                    #[wrap(Some)]
                    set_child = &gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 16,
                        set_margin_all: 8,

                        // For each group of duplicates
                        #[local_ref]
                        duplicate_groups -> gtk::Box {
                        }
                    },
                },
            },
        }
    }

    async fn init(
        init: Self::Init,
        root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        let model = Self {
            duplicates: init.duplicates.clone(),
            file_data_source: init.file_data_source,
        };

        let duplicate_groups = gtk::Box::new(gtk::Orientation::Vertical, 16);

        // Create the duplicate groups dynamically
        for (i, group) in model.duplicates.iter().enumerate() {
            if !group.is_empty() {
                // Create a frame for this group
                let frame = gtk::Frame::new(Some(&format!("Group {} - Fingerprint: {}...", i+1, &group[0].fingerprint[..16])));
                frame.add_css_class("card");

                // Create a box for the group content
                let group_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
                group_box.set_margin_all(12);

                // Add each file in the group
                for file in group {
                    let file_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);

                    // Add a delete button
                    let delete_button = gtk::Button::new();
                    delete_button.set_icon_name("user-trash-symbolic");
                    delete_button.add_css_class("destructive-action");
                    delete_button.set_tooltip_text(Some("Delete this file"));

                    // Clone the file for the closure
                    let file_clone = file.clone();
                    let sender_clone = sender.clone();

                    delete_button.connect_clicked(move |_| {
                        sender_clone.input(DuplicatesDialogInput::DeleteFile(file_clone.clone()));
                    });

                    file_box.append(&delete_button);

                    // Add file information
                    let file_info = gtk::Box::new(gtk::Orientation::Vertical, 4);
                    file_info.set_hexpand(true);

                    let path_label = gtk::Label::new(Some(&file.path));
                    path_label.set_halign(gtk::Align::Start);
                    path_label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
                    path_label.set_tooltip_text(Some(&file.path));
                    file_info.append(&path_label);

                    let details_label = gtk::Label::new(Some(&format!("Size: {} bytes, Type: {}", file.size, file.type_)));
                    details_label.set_halign(gtk::Align::Start);
                    details_label.add_css_class("dim-label");
                    file_info.append(&details_label);

                    file_box.append(&file_info);
                    group_box.append(&file_box);
                }

                frame.set_child(Some(&group_box));
                duplicate_groups.append(&frame);
            }
        }

        let widgets = view_output!();

        AsyncComponentParts { model, widgets }
    }

    async fn update(
        &mut self,
        msg: Self::Input,
        sender: AsyncComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match msg {
            DuplicatesDialogInput::DeleteFile(file) => {
                // Show a confirmation dialog
                let dialog = gtk::MessageDialog::new(
                    gtk::gio::Application::default()
                        .and_then(|app| app.downcast::<gtk::Application>().ok())
                        .and_then(|app| app.active_window()).as_ref(),
                    gtk::DialogFlags::MODAL,
                    gtk::MessageType::Warning,
                    gtk::ButtonsType::YesNo,
                    &format!("Are you sure you want to delete this file?\n\n{}", file.path)
                );
                dialog.set_title(Some("Confirm Deletion"));

                let file_clone = file.clone();
                let sender_clone = sender.clone();

                dialog.connect_response(move |dialog, response| {
                    if response == gtk::ResponseType::Yes {
                        sender_clone.input(DuplicatesDialogInput::ConfirmDeleteFile(file_clone.clone()));
                    }
                    dialog.close();
                });

                dialog.show();
            }
            DuplicatesDialogInput::ConfirmDeleteFile(file) => {
                // Create a loading dialog
                let dialog = gtk::MessageDialog::new(
                    gtk::gio::Application::default()
                        .and_then(|app| app.downcast::<gtk::Application>().ok())
                        .and_then(|app| app.active_window()).as_ref(),
                    gtk::DialogFlags::MODAL,
                    gtk::MessageType::Info,
                    gtk::ButtonsType::None,
                    "Deleting file..."
                );
                dialog.set_title(Some("Deleting File"));
                dialog.show();

                // Execute the deletion directly
                let result = self.file_data_source.delete_file(file.clone()).await;

                // Close the loading dialog
                dialog.close();

                match result {
                    Ok(_) => {
                        // Show success message
                        let success_dialog = gtk::MessageDialog::new(
                            gtk::gio::Application::default()
                                .and_then(|app| app.downcast::<gtk::Application>().ok())
                                .and_then(|app| app.active_window()).as_ref(),
                            gtk::DialogFlags::MODAL,
                            gtk::MessageType::Info,
                            gtk::ButtonsType::Ok,
                            "File deleted successfully"
                        );
                        success_dialog.set_title(Some("File Deleted"));
                        success_dialog.connect_response(|dialog, _| {
                            dialog.close();
                        });
                        success_dialog.show();

                        // Notify the parent component that a file was deleted
                        sender.output(DuplicatesDialogOutput::FileDeleted).unwrap();

                        // Remove the deleted file from our duplicates list
                        for group in &mut self.duplicates {
                            group.retain(|f| f.id != file.id);
                        }

                        // Remove any empty groups
                        self.duplicates.retain(|group| !group.is_empty());

                        // If no more duplicates, close the dialog
                        if self.duplicates.is_empty() {
                            if let Some(dialog) = gtk::gio::Application::default()
                                .and_then(|app| app.downcast::<gtk::Application>().ok())
                                .and_then(|app| app.active_window()) {
                                if let Some(dialog) = dialog.downcast_ref::<gtk::Dialog>() {
                                    dialog.close();
                                }
                            }
                        }
                    },
                    Err(e) => {
                        // Show error message
                        let error_dialog = gtk::MessageDialog::new(
                            gtk::gio::Application::default()
                                .and_then(|app| app.downcast::<gtk::Application>().ok())
                                .and_then(|app| app.active_window()).as_ref(),
                            gtk::DialogFlags::MODAL,
                            gtk::MessageType::Error,
                            gtk::ButtonsType::Ok,
                            &format!("Error deleting file: {}", e)
                        );
                        error_dialog.set_title(Some("Error"));
                        error_dialog.connect_response(|dialog, _| {
                            dialog.close();
                        });
                        error_dialog.show();
                    }
                }
            }
        }
    }
}
