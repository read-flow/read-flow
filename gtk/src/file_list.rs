use gtk::prelude::*;
use relm4::RelmWidgetExt;
use relm4::component::AsyncComponent;
use relm4::component::AsyncComponentParts;
use relm4::component::AsyncComponentSender;
use relm4::gtk;
use relm4::once_cell::sync::Lazy;
use relm4::prelude::*;

use archive_organizer::api::File;
use archive_organizer::api::FileDataSource;

use crate::file_box::FileBox;
use crate::file_box::FileBoxOutput;
use crate::file_details::{FileDetails, FileDetailsOutput};

use tracing;

const COMPONENT_CSS: &str = include_str!("../assets/style.css");

/// The initializer for the CSS, ensuring it only happens once.
static INITIALIZE_CSS: Lazy<()> = Lazy::new(|| {
    relm4::set_global_css_with_priority(COMPONENT_CSS, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
});

pub struct FileList<FDS>
where
    FileDetails<FDS>: relm4::component::AsyncComponent,
{
    pub(super) file_data_source: FDS,
    files: AsyncFactoryVecDeque<FileBox>,
    details: Option<AsyncController<FileDetails<FDS>>>,
}

#[derive(Debug)]
pub enum FileListInput {
    FileClicked(File),
    RefreshFiles,
}

#[relm4::component(pub, async)]
impl<FDS> AsyncComponent for FileList<FDS>
where
    FDS: FileDataSource + Clone + 'static,
{
    type Init = FDS;
    type Input = FileListInput;
    type Output = FileBoxOutput;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::ScrolledWindow {
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 8,
                set_margin_all: 12,

                #[local_ref]
                files_box -> gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 8,
                },
            },
        },
    }

    async fn init(
        file_data_source: Self::Init,
        root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        tracing::debug!("Initializing FileList component");

        // Initialize the CSS.
        #[allow(clippy::no_effect)]
        *INITIALIZE_CSS;

        tracing::debug!("Loading files from data source");
        let files = match file_data_source.get_files().await {
            Ok(files) => {
                tracing::debug!("Successfully loaded {} files", files.len());
                files
            }
            Err(e) => {
                tracing::error!("Error loading files: {}", e);
                Vec::new()
            }
        };

        tracing::debug!("Setting up file list factory");
        let mut files_deque = AsyncFactoryVecDeque::builder()
            .launch(gtk::Box::default())
            .forward(sender.input_sender(), |output| match output {
                FileBoxOutput::FileClicked(file) => FileListInput::FileClicked(file),
            });

        {
            let mut mut_files = files_deque.guard();
            tracing::debug!("Adding {} files to list", files.len());
            for file in files {
                mut_files.push_back(file);
            }
        }

        let model = FileList {
            file_data_source,
            files: files_deque,
            details: None,
        };

        let files_box = model.files.widget();
        let widgets = view_output!();
        tracing::debug!("FileList initialization complete");

        AsyncComponentParts { model, widgets }
    }

    async fn update(
        &mut self,
        msg: Self::Input,
        sender: AsyncComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match msg {
            FileListInput::FileClicked(file) => {
                if let Some(_details) = self.details.take() {
                    // The component will be dropped when the window is closed
                }
                self.details = Some(
                    FileDetails::builder()
                        .launch((file.clone(), self.file_data_source.clone()))
                        .forward(sender.input_sender(), |msg| match msg {
                            FileDetailsOutput::TagsChanged(_) => FileListInput::RefreshFiles,
                        }),
                );
                sender.output(FileBoxOutput::FileClicked(file)).unwrap();
            }
            FileListInput::RefreshFiles => {
                // Reload files from the data source
                match self.file_data_source.get_files().await {
                    Ok(files) => {
                        // Clear and repopulate the files list
                        let mut mut_files = self.files.guard();
                        mut_files.clear();
                        for file in files {
                            mut_files.push_back(file);
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Error refreshing files: {}", e);
                    }
                }
            }
        }
    }
}
