// SPDX-License-Identifier: GPL-3.0-or-later

use cosmic::Action;
use cosmic::Element;
use cosmic::Task;
use cosmic::iced::ContentFit;
use cosmic::iced::Length;
use cosmic::iced::widget::scrollable;
use cosmic::widget;

use super::traits::Page;
use crate::ICON_SIZE;
use crate::app::ContextView;
use crate::fl;

/// Image content to display in the image viewer page.
#[derive(Debug, Clone)]
pub enum ViewerImage {
    Raster(widget::image::Handle),
    Svg(widget::svg::Handle),
}

#[derive(Debug, Clone)]
pub enum ImageViewerOutput {
    Close(u64),
}

#[derive(Debug, Clone)]
pub enum ImageViewerMessage {
    Out(ImageViewerOutput),
}

pub struct ImageViewer {
    id: u64,
    image: ViewerImage,
}

impl ImageViewer {
    pub fn new(id: u64, image: ViewerImage) -> Self {
        Self { id, image }
    }

    pub fn display_name(&self) -> String {
        fl!("epub-viewer-image-viewer-title")
    }
}

impl Page for ImageViewer {
    type Message = ImageViewerMessage;

    fn view(&self) -> Element<'_, ImageViewerMessage> {
        let image: Element<'_, ImageViewerMessage> = match &self.image {
            ViewerImage::Raster(handle) => widget::image(handle)
                .content_fit(ContentFit::Contain)
                .into(),
            ViewerImage::Svg(handle) => widget::svg(handle.clone())
                .content_fit(ContentFit::Contain)
                .into(),
        };

        widget::scrollable(image)
            .direction(scrollable::Direction::Both {
                vertical: scrollable::Scrollbar::new(),
                horizontal: scrollable::Scrollbar::new(),
            })
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn view_header_end(&self) -> Vec<Element<'_, ImageViewerMessage>> {
        vec![
            widget::button::icon(widget::icon::from_name("window-close-symbolic").size(ICON_SIZE))
                .on_press(ImageViewerMessage::Out(ImageViewerOutput::Close(self.id)))
                .tooltip(fl!("epub-viewer-back"))
                .into(),
        ]
    }

    fn update(&mut self, message: ImageViewerMessage) -> Task<Action<ImageViewerMessage>> {
        match message {
            ImageViewerMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent component")
            }
        }
    }

    fn view_context(&self) -> ContextView<'_, ImageViewerMessage> {
        ContextView {
            title: self.display_name(),
            content: widget::text("").into(),
        }
    }
}
