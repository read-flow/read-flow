use gtk::prelude::*;
use relm4::RelmWidgetExt;
use relm4::component::AsyncComponent;
use relm4::component::AsyncComponentParts;
use relm4::component::AsyncComponentSender;
use relm4::gtk;

use archive_organizer::api::{File, FileDataSource};

#[derive(Debug)]
pub enum DuplicatesPageInput {
    DeleteFile(File),
    ConfirmDeleteFile(File),
    RefreshDuplicates,
}

#[derive(Debug)]
pub enum DuplicatesPageOutput {
    FileDeleted,
    Close,
    Refreshed,
}

#[derive(Debug)]
pub struct DuplicatesPageInit<FDS> {
    pub duplicates: Vec<Vec<File>>,
    pub file_data_source: FDS,
    pub source_name: String,
}

pub struct DuplicatesPage<FDS> {
    duplicates: Vec<Vec<File>>,
    file_data_source: FDS,
    source_name: String,
}

impl<FDS> DuplicatesPage<FDS>
where
    FDS: FileDataSource + Clone + Send + Sync + 'static,
{
    /// Rebuilds the duplicate groups UI with the current duplicates data
    fn rebuild_duplicate_groups(&self, root: &gtk::Box, sender: &AsyncComponentSender<Self>) {
        // Find the scrolled window that contains the duplicate groups
        if let Some(scrolled_window) = root.last_child() {
            if let Some(scrolled) = scrolled_window.downcast_ref::<gtk::ScrolledWindow>() {
                // Get the viewport inside the scrolled window
                if let Some(viewport) = scrolled.child() {
                    if let Some(viewport_widget) = viewport.downcast_ref::<gtk::Viewport>() {
                        // Create a new box for the duplicate groups
                        let duplicate_groups = gtk::Box::new(gtk::Orientation::Vertical, 16);
                        duplicate_groups.set_margin_all(8);

                        // Create the duplicate groups dynamically
                        for (i, group) in self.duplicates.iter().enumerate() {
                            if !group.is_empty() {
                                // Create a frame for this group
                                let frame = gtk::Frame::new(Some(&format!("Group {} - Fingerprint: {}...", i+1, &group[0].fingerprint[..16])));
                                frame.add_css_class("card");

                                // Create a box for the group content
                                let group_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
                                group_box.set_margin_all(12);

                                // Add a label with the number of duplicates
                                let label = gtk::Label::new(Some(&format!("{} duplicate files found with the same content:", group.len())));
                                label.set_halign(gtk::Align::Start);
                                label.add_css_class("heading");
                                group_box.append(&label);

                                // Add a separator
                                let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
                                separator.set_margin_top(4);
                                separator.set_margin_bottom(8);
                                group_box.append(&separator);

                                // Create a list box for the files
                                let list_box = gtk::ListBox::new();
                                list_box.add_css_class("boxed-list");
                                list_box.set_selection_mode(gtk::SelectionMode::None);

                                // Add each file to the list
                                for file in group {
                                    let row = gtk::ListBoxRow::new();

                                    let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                                    row_box.set_margin_all(8);

                                    // File information
                                    let file_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
                                    file_box.set_hexpand(true);
                                    file_box.set_vexpand(false);
                                    file_box.set_width_request(300); // Minimum width
                                    file_box.set_margin_end(8);

                                    // Extract filename and directory from path
                                    let (filename, folder) = crate::ui_utils::extract_path_components(&file.path);

                                    // Create a box for the filename and folder
                                    let path_box = gtk::Box::new(gtk::Orientation::Vertical, 2);

                                    // Filename label (bold)
                                    let filename_label = gtk::Label::new(Some(&filename));
                                    filename_label.set_halign(gtk::Align::Start);
                                    filename_label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
                                    // Don't set max_width_chars to allow dynamic resizing
                                    filename_label.set_hexpand(true);
                                    filename_label.set_xalign(0.0); // Left align text
                                    filename_label.add_css_class("heading");
                                    path_box.append(&filename_label);

                                    // Full path label (smaller, dimmed)
                                    let full_path_label = gtk::Label::new(Some(&file.path));
                                    full_path_label.set_halign(gtk::Align::Start);
                                    full_path_label.set_ellipsize(gtk::pango::EllipsizeMode::Start);
                                    // Don't set max_width_chars to allow dynamic resizing
                                    full_path_label.set_hexpand(true);
                                    full_path_label.set_xalign(0.0); // Left align text
                                    full_path_label.set_wrap(true);
                                    full_path_label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
                                    full_path_label.add_css_class("dim-label");
                                    full_path_label.add_css_class("caption");
                                    path_box.append(&full_path_label);

                                    file_box.append(&path_box);

                                    let details_label = gtk::Label::new(Some(&format!(
                                        "Type: {}, Size: {} bytes, Status: {:?}",
                                        file.type_, file.size, file.status
                                    )));
                                    details_label.set_halign(gtk::Align::Start);
                                    details_label.add_css_class("dim-label");
                                    file_box.append(&details_label);

                                    row_box.append(&file_box);

                                    // Delete button
                                    let delete_button = gtk::Button::new();
                                    delete_button.set_icon_name("user-trash-symbolic");
                                    delete_button.set_tooltip_text(Some("Delete this file"));
                                    delete_button.add_css_class("destructive-action");

                                    // Create a clone of the file for the closure
                                    let file_clone = file.clone();

                                    // Connect the button to the delete action
                                    let sender_clone = sender.clone();
                                    delete_button.connect_clicked(move |_| {
                                        sender_clone.input(DuplicatesPageInput::DeleteFile(file_clone.clone()));
                                    });

                                    row_box.append(&delete_button);

                                    row.set_child(Some(&row_box));
                                    list_box.append(&row);
                                }

                                group_box.append(&list_box);
                                frame.set_child(Some(&group_box));
                                duplicate_groups.append(&frame);
                            }
                        }

                        // Add the new duplicate groups box to the viewport
                        viewport_widget.set_child(Some(&duplicate_groups));
                    }
                }
            }
        }
    }
}

#[relm4::component(pub, async)]
impl<FDS> AsyncComponent for DuplicatesPage<FDS>
where
    FDS: FileDataSource + Clone + Send + Sync + 'static,
{
    type Init = DuplicatesPageInit<FDS>;
    type Input = DuplicatesPageInput;
    type Output = DuplicatesPageOutput;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 8,
            set_margin_all: 16,

            // Header with title and close button
            append = &gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                set_margin_bottom: 16,

                append = &gtk::Label {
                    set_markup: &format!("<b>Duplicate Files from {}</b>", model.source_name),
                    set_halign: gtk::Align::Start,
                    set_hexpand: true,
                },

                append = &gtk::Button {
                    set_icon_name: "view-refresh-symbolic",
                    set_tooltip_text: Some("Refresh"),
                    add_css_class: "flat",
                    add_css_class: "circular",
                    connect_clicked[sender] => move |_| {
                        sender.input(DuplicatesPageInput::RefreshDuplicates);
                    },
                },
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
            source_name: init.source_name,
        };

        // Create an empty box for the duplicate groups
        // It will be populated in the view! macro and later by rebuild_duplicate_groups
        let duplicate_groups = gtk::Box::new(gtk::Orientation::Vertical, 16);

        let widgets = view_output!();

        // After the view is created, populate the duplicate groups
        // We need to do this after view_output! because it needs the widget hierarchy to be set up
        model.rebuild_duplicate_groups(&root, &sender);

        AsyncComponentParts { model, widgets }
    }

    async fn update(
        &mut self,
        msg: Self::Input,
        sender: AsyncComponentSender<Self>,
        root: &Self::Root,
    ) {
        match msg {
            DuplicatesPageInput::DeleteFile(file) => {
                // Show a confirmation dialog
                let dialog = gtk::MessageDialog::new(
                    gtk::gio::Application::default()
                        .and_then(|app| app.downcast::<gtk::Application>().ok())
                        .and_then(|app| app.active_window())
                        .as_ref(),
                    gtk::DialogFlags::MODAL,
                    gtk::MessageType::Warning,
                    gtk::ButtonsType::YesNo,
                    format!(
                        "Are you sure you want to delete this file?\n\n{}",
                        file.path
                    ),
                );
                dialog.set_title(Some("Confirm Deletion"));

                let file_clone = file.clone();
                let sender_clone = sender.clone();

                dialog.connect_response(move |dialog, response| {
                    if response == gtk::ResponseType::Yes {
                        sender_clone
                            .input(DuplicatesPageInput::ConfirmDeleteFile(file_clone.clone()));
                    }
                    dialog.close();
                });

                dialog.show();
            }
            DuplicatesPageInput::ConfirmDeleteFile(file) => {
                // Create a loading dialog
                let dialog = gtk::MessageDialog::new(
                    gtk::gio::Application::default()
                        .and_then(|app| app.downcast::<gtk::Application>().ok())
                        .and_then(|app| app.active_window())
                        .as_ref(),
                    gtk::DialogFlags::MODAL,
                    gtk::MessageType::Info,
                    gtk::ButtonsType::None,
                    "Deleting file...",
                );
                dialog.set_title(Some("Deleting File"));
                dialog.show();

                // Attempt to delete the file
                match self.file_data_source.delete_file(file.clone()).await {
                    Ok(_) => {
                        dialog.close();

                        // Show success message
                        let success_dialog = gtk::MessageDialog::new(
                            gtk::gio::Application::default()
                                .and_then(|app| app.downcast::<gtk::Application>().ok())
                                .and_then(|app| app.active_window())
                                .as_ref(),
                            gtk::DialogFlags::MODAL,
                            gtk::MessageType::Info,
                            gtk::ButtonsType::Ok,
                            "File deleted successfully",
                        );
                        success_dialog.set_title(Some("Success"));
                        success_dialog.connect_response(|dialog, _| {
                            dialog.close();
                        });
                        success_dialog.show();

                        // Notify parent that a file was deleted
                        sender.output(DuplicatesPageOutput::FileDeleted).unwrap();

                        // Refresh the duplicates list
                        sender.input(DuplicatesPageInput::RefreshDuplicates);
                    }
                    Err(e) => {
                        dialog.close();

                        // Show error message
                        let error_dialog = gtk::MessageDialog::new(
                            gtk::gio::Application::default()
                                .and_then(|app| app.downcast::<gtk::Application>().ok())
                                .and_then(|app| app.active_window())
                                .as_ref(),
                            gtk::DialogFlags::MODAL,
                            gtk::MessageType::Error,
                            gtk::ButtonsType::Ok,
                            format!("Failed to delete file: {}", e),
                        );
                        error_dialog.set_title(Some("Error"));
                        error_dialog.connect_response(|dialog, _| {
                            dialog.close();
                        });
                        error_dialog.show();
                    }
                }
            }
            DuplicatesPageInput::RefreshDuplicates => {
                tracing::debug!("Refreshing duplicates list");

                // Create a loading dialog
                let dialog = gtk::MessageDialog::new(
                    gtk::gio::Application::default()
                        .and_then(|app| app.downcast::<gtk::Application>().ok())
                        .and_then(|app| app.active_window())
                        .as_ref(),
                    gtk::DialogFlags::MODAL,
                    gtk::MessageType::Info,
                    gtk::ButtonsType::None,
                    "Refreshing duplicates list..."
                );
                dialog.set_title(Some("Refreshing"));
                dialog.show();

                // Get the file list from the data source
                match self.file_data_source.get_files().await {
                    Ok(files) => {
                        // Group files by fingerprint
                        let buckets = archive_organizer::to_buckets(
                            files.iter(),
                            |file| file.fingerprint.clone()
                        );

                        // Filter for buckets with more than one file (duplicates)
                        let new_duplicates: Vec<Vec<File>> = buckets
                            .into_iter()
                            .filter(|(_, files)| files.len() > 1)
                            .map(|(_, files)| files.into_iter().cloned().collect())
                            .collect();

                        // Update the duplicates list
                        self.duplicates = new_duplicates;

                        // Close the dialog
                        dialog.close();

                        // Rebuild the UI with the new duplicates
                        self.rebuild_duplicate_groups(root, &sender);

                        // Notify the parent that we need to rebuild the UI
                        sender.output(DuplicatesPageOutput::Refreshed).unwrap();
                    }
                    Err(e) => {
                        // Close the loading dialog
                        dialog.close();

                        // Show error message
                        let error_dialog = gtk::MessageDialog::new(
                            gtk::gio::Application::default()
                                .and_then(|app| app.downcast::<gtk::Application>().ok())
                                .and_then(|app| app.active_window())
                                .as_ref(),
                            gtk::DialogFlags::MODAL,
                            gtk::MessageType::Error,
                            gtk::ButtonsType::Ok,
                            &format!("Failed to refresh duplicates list: {}", e)
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
