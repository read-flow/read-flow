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
use crate::file_details::FileDetails;

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
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 8,
            set_margin_all: 12,

            #[local_ref]
            files_box -> gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 8,
            },
        }
    }

    async fn init(
        file_data_source: Self::Init,
        root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        // Initialize the CSS.
        #[allow(clippy::no_effect)]
        *INITIALIZE_CSS;

        let files = match file_data_source.get_files().await {
            Ok(files) => files,
            Err(e) => {
                eprintln!("Error loading files: {}", e);
                Vec::new()
            }
        };

        let mut files_deque = AsyncFactoryVecDeque::builder()
            .launch(gtk::Box::default())
            .forward(sender.input_sender(), |output| match output {
                FileBoxOutput::FileClicked(file) => FileListInput::FileClicked(file),
            });

        {
            let mut mut_files = files_deque.guard();
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
                        .detach(),
                );
                sender.output(FileBoxOutput::FileClicked(file)).unwrap();
            }
        }
    }
}
