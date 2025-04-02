use gtk::prelude::*;
use relm4::RelmWidgetExt;
use relm4::component::AsyncComponent;
use relm4::component::AsyncComponentController;
use relm4::component::AsyncComponentParts;
use relm4::component::AsyncComponentSender;
use relm4::component::AsyncController;
use relm4::gtk;
use relm4::loading_widgets::LoadingWidgets;
use relm4::once_cell::sync::Lazy;
use relm4::view;

use archive_organizer::api::File;
use archive_organizer::api::FileDataSource;

use crate::file_box::FileBoxOutput;
use crate::file_details::FileDetails;
use crate::file_list::FileList;

use std::sync::Arc;

const COMPONENT_CSS: &str = include_str!("../assets/style.css");

/// The initializer for the CSS, ensuring it only happens once.
static INITIALIZE_CSS: Lazy<()> = Lazy::new(|| {
    relm4::set_global_css_with_priority(COMPONENT_CSS, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
});

pub struct App<FDS>
where
    FileList<FDS>: relm4::component::AsyncComponent,
    FileDetails<FDS>: relm4::component::AsyncComponent,
{
    file_data_source: Arc<FDS>,
    file_list: AsyncController<FileList<FDS>>,
    details: Option<AsyncController<FileDetails<FDS>>>,
}

#[derive(Debug)]
pub enum AppInput {
    FileClicked(File),
}

#[relm4::component(pub, async)]
impl<FDS> AsyncComponent for App<FDS>
where
    FDS: FileDataSource + 'static,
{
    type Init = Arc<FDS>;
    type Input = AppInput;
    type Output = ();
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Window {
            set_title: Some("Archive Organizer"),
            set_default_width: 800,
            set_default_height: 600,
            set_icon_name: Some("folder-archives"),

            gtk::HeaderBar {
                set_show_title_buttons: true,
                set_title_widget: Some(&gtk::Label::new(Some("Archive Organizer"))),
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 12,
                set_margin_all: 12,

                gtk::ScrolledWindow {
                    set_vexpand: true,
                    set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),

                    #[local_ref]
                    file_list -> gtk::Box,
                },
            },
        }
    }

    fn init_loading_widgets(root: Self::Root) -> Option<LoadingWidgets> {
        view! {
            #[local]
            root {
                set_title: Some("Archive Organizer"),
                set_default_size: (800, 600),

                #[name(spinner)]
                gtk::Spinner {
                    start: (),
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,
                }
            }
        }
        Some(LoadingWidgets::new(root, spinner))
    }

    async fn init(
        file_data_source: Self::Init,
        root: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        // Initialize the CSS.
        #[allow(clippy::no_effect)]
        *INITIALIZE_CSS;

        let file_list = FileList::builder()
            .launch(file_data_source.clone())
            .forward(sender.input_sender(), |output| match output {
                FileBoxOutput::FileClicked(file) => AppInput::FileClicked(file),
            });

        let model = App {
            file_data_source,
            file_list,
            details: None,
        };

        let file_list = model.file_list.widget();

        let widgets = view_output!();

        AsyncComponentParts { model, widgets }
    }

    async fn update(
        &mut self,
        msg: Self::Input,
        _sender: AsyncComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match msg {
            AppInput::FileClicked(file) => {
                if let Some(_details) = self.details.take() {
                    // The component will be dropped when the window is closed
                }
                self.details = Some(
                    FileDetails::builder()
                        .launch((file.clone(), self.file_data_source.clone()))
                        .detach(),
                );
            }
        }
    }
}
