// SPDX-License-Identifier: GPL-3.0-or-later

use cosmic::Action;
use cosmic::Application;
use cosmic::Apply;
use cosmic::Element;
use cosmic::Task;
use cosmic::cosmic_config;
use cosmic::cosmic_config::CosmicConfigEntry;
use cosmic::cosmic_theme;
use cosmic::iced::Length;
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::alignment::Vertical;
use cosmic::theme;
use cosmic::widget;

use crate::ICON_SIZE;
use crate::app::ContextView;
use crate::app::ReadFlow;
use crate::config::Config;
use crate::config::EpubViewerConfig;
use crate::fl;
use crate::layout::layout;
use crate::page::Page;

pub struct AppSettingsPage {
    config: Config,
}

#[derive(Debug, Clone)]
pub enum AppSettingsMessage {
    SetEpubViewer(EpubViewerConfig),
}

impl AppSettingsPage {
    pub fn new(config: Config) -> (Self, Task<Action<AppSettingsMessage>>) {
        (Self { config }, Task::none())
    }

    pub fn update_config(&mut self, config: Config) {
        self.config = config;
    }
}

impl Page for AppSettingsPage {
    type Message = AppSettingsMessage;

    fn view(&self) -> Element<'_, AppSettingsMessage> {
        let cosmic_theme::Spacing { space_xs, .. } = theme::active().cosmic().spacing;
        let viewer_section = widget::settings::section()
            .title(fl!("settings-viewer-section"))
            .add(
                widget::settings::item::builder(fl!("settings-epub-viewer"))
                    .description(fl!("settings-epub-viewer-description"))
                    .icon(widget::icon::from_name("application-epub+zip").size(ICON_SIZE))
                    .control(
                        widget::Column::from_vec(vec![
                            widget::radio(
                                widget::text::body(fl!("settings-epub-viewer-native")),
                                EpubViewerConfig::NativeEpub,
                                Some(self.config.epub_viewer),
                                AppSettingsMessage::SetEpubViewer,
                            )
                            .into(),
                            widget::radio(
                                widget::text::body(fl!("settings-epub-viewer-mupdf")),
                                EpubViewerConfig::MuPdf,
                                Some(self.config.epub_viewer),
                                AppSettingsMessage::SetEpubViewer,
                            )
                            .into(),
                            widget::radio(
                                widget::text::body(fl!("settings-epub-viewer-external")),
                                EpubViewerConfig::ExternalViewer,
                                Some(self.config.epub_viewer),
                                AppSettingsMessage::SetEpubViewer,
                            )
                            .into(),
                        ])
                        .spacing(space_xs)
                        .align_x(Horizontal::Left),
                    ),
            );

        layout(widget::settings::view_column(vec![viewer_section.into()]))
            .apply(widget::scrollable::vertical)
            .apply(widget::container)
            .height(Length::Fill)
            .align_x(Horizontal::Center)
            .align_y(Vertical::Top)
            .into()
    }

    fn view_context(&self) -> ContextView<'_, AppSettingsMessage> {
        ContextView {
            title: fl!("app-settings-page-title"),
            content: widget::text("").into(),
        }
    }

    fn update(&mut self, message: AppSettingsMessage) -> Task<Action<AppSettingsMessage>> {
        match message {
            AppSettingsMessage::SetEpubViewer(epub_viewer) => {
                self.config.epub_viewer = epub_viewer;
                if let Ok(ctx) = cosmic_config::Config::new(ReadFlow::APP_ID, Config::VERSION) {
                    let _ = self.config.write_entry(&ctx);
                }
                Task::none()
            }
        }
    }
}
