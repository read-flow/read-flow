// SPDX-License-Identifier: GPL-3.0-or-later

use std::cell::Cell;

use cosmic::Action;
use cosmic::Element;
use cosmic::Task;
use cosmic::cosmic_theme;
use cosmic::iced::ContentFit;
use cosmic::iced::Length;
use cosmic::iced::widget::scrollable;
use cosmic::theme;
use cosmic::widget;

use super::traits::Page;
use crate::ICON_SIZE;
use crate::app::ContextView;
use crate::fl;

/// Image content to display in the image viewer page.
#[derive(Debug, Clone)]
pub enum ViewerImage {
    Raster {
        handle: widget::image::Handle,
        natural_width: u32,
        natural_height: u32,
    },
    Svg(widget::svg::Handle),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Zoom {
    FitBoth,
    FitHeight,
    FitWidth,
    Percent(i16),
}

impl Zoom {
    fn all() -> &'static [Self] {
        &[
            Zoom::FitBoth,
            Zoom::FitHeight,
            Zoom::FitWidth,
            Zoom::Percent(25),
            Zoom::Percent(50),
            Zoom::Percent(75),
            Zoom::Percent(100),
            Zoom::Percent(125),
            Zoom::Percent(150),
            Zoom::Percent(200),
            Zoom::Percent(300),
            Zoom::Percent(400),
            Zoom::Percent(500),
            Zoom::Percent(600),
            Zoom::Percent(800),
        ]
    }

    fn percents() -> impl Iterator<Item = i16> {
        Self::all().iter().filter_map(|z| {
            if let Zoom::Percent(p) = z {
                Some(*p)
            } else {
                None
            }
        })
    }
}

#[derive(Debug, Clone)]
pub enum ImageViewerOutput {
    Close(u64),
}

#[derive(Debug, Clone)]
pub enum ImageViewerMessage {
    Out(ImageViewerOutput),
    ZoomDropdown(usize),
    ZoomIn,
    ZoomOut,
}

pub struct ImageViewer {
    id: u64,
    image: ViewerImage,
    zoom: Zoom,
    zoom_names: Vec<String>,
    /// Effective scale last computed in `view()`. Used to step through percent levels.
    view_ratio: Cell<f32>,
}

impl ImageViewer {
    pub fn new(id: u64, image: ViewerImage) -> Self {
        let zoom_names = Zoom::all()
            .iter()
            .map(|z| match z {
                Zoom::FitBoth => fl!("epub-viewer-image-zoom-fit-both"),
                Zoom::FitHeight => fl!("epub-viewer-image-zoom-fit-height"),
                Zoom::FitWidth => fl!("epub-viewer-image-zoom-fit-width"),
                Zoom::Percent(p) => format!("{p}%"),
            })
            .collect();

        Self {
            id,
            image,
            zoom: Zoom::FitBoth,
            zoom_names,
            view_ratio: Cell::new(1.0),
        }
    }

    pub fn display_name(&self) -> String {
        fl!("epub-viewer-image-viewer-title")
    }
}

impl Page for ImageViewer {
    type Message = ImageViewerMessage;

    fn view(&self) -> Element<'_, ImageViewerMessage> {
        let zoom = self.zoom;
        let image = &self.image;
        let view_ratio = &self.view_ratio;

        widget::responsive(move |viewport| {
            // Compute natural dimensions (or viewport-relative for SVG).
            let (nat_w, nat_h) = match image {
                ViewerImage::Raster {
                    natural_width,
                    natural_height,
                    ..
                } => (*natural_width as f32, *natural_height as f32),
                ViewerImage::Svg(_) => (viewport.width, viewport.height),
            };

            let ratio = if nat_w > 0.0 && nat_h > 0.0 {
                match zoom {
                    Zoom::FitBoth => (viewport.width / nat_w).min(viewport.height / nat_h),
                    Zoom::FitHeight => viewport.height / nat_h,
                    Zoom::FitWidth => viewport.width / nat_w,
                    Zoom::Percent(p) => p as f32 / 100.0,
                }
            } else {
                1.0
            };
            view_ratio.set(ratio);

            let img_w = nat_w * ratio;
            let img_h = nat_h * ratio;

            let img: Element<'_, ImageViewerMessage> = match image {
                ViewerImage::Raster { handle, .. } => widget::image(handle)
                    .content_fit(ContentFit::Fill)
                    .width(img_w)
                    .height(img_h)
                    .into(),
                // SVG has no fixed natural resolution, so use Contain to preserve
                // aspect ratio. The container size (img_w × img_h) is viewport-relative,
                // making 100% == FitBoth and 200% == 2× that rendered size.
                ViewerImage::Svg(handle) => widget::svg(handle.clone())
                    .content_fit(ContentFit::Contain)
                    .width(img_w)
                    .height(img_h)
                    .into(),
            };

            // Center the image when smaller than the viewport;
            // the scrollable handles overflow when the image is larger.
            let centered = widget::container(img)
                .center_x(Length::Fill)
                .center_y(Length::Fill);

            widget::scrollable(centered)
                .direction(scrollable::Direction::Both {
                    vertical: scrollable::Scrollbar::new(),
                    horizontal: scrollable::Scrollbar::new(),
                })
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        })
        .into()
    }

    fn view_header_start(&self) -> Vec<Element<'_, ImageViewerMessage>> {
        let cosmic_theme::Spacing { space_xxs, .. } = theme::active().cosmic().spacing;
        vec![
            widget::button::icon(widget::icon::from_name("go-previous-symbolic").size(ICON_SIZE))
                .on_press(ImageViewerMessage::Out(ImageViewerOutput::Close(self.id)))
                .tooltip(fl!("epub-viewer-back"))
                .padding(space_xxs)
                .into(),
        ]
    }

    fn view_header_end(&self) -> Vec<Element<'_, ImageViewerMessage>> {
        let cosmic_theme::Spacing { space_xxs, .. } = theme::active().cosmic().spacing;
        let ratio = self.view_ratio.get();
        let can_zoom_out = Zoom::percents().any(|p| p as f32 / 100.0 < ratio);
        let can_zoom_in = Zoom::percents().any(|p| p as f32 / 100.0 > ratio);

        vec![
            widget::button::icon(widget::icon::from_name("zoom-out-symbolic").size(ICON_SIZE))
                .on_press_maybe(can_zoom_out.then_some(ImageViewerMessage::ZoomOut))
                .tooltip(fl!("epub-viewer-image-zoom-out"))
                .padding(space_xxs)
                .into(),
            widget::button::icon(widget::icon::from_name("zoom-in-symbolic").size(ICON_SIZE))
                .on_press_maybe(can_zoom_in.then_some(ImageViewerMessage::ZoomIn))
                .tooltip(fl!("epub-viewer-image-zoom-in"))
                .padding(space_xxs)
                .into(),
        ]
    }

    fn update(&mut self, message: ImageViewerMessage) -> Task<Action<ImageViewerMessage>> {
        match message {
            ImageViewerMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent component")
            }
            ImageViewerMessage::ZoomDropdown(index) => {
                if let Some(&zoom) = Zoom::all().get(index) {
                    self.zoom = zoom;
                }
                Task::none()
            }
            ImageViewerMessage::ZoomIn => {
                let ratio = self.view_ratio.get();
                if let Some(p) = Zoom::percents().find(|&p| p as f32 / 100.0 > ratio) {
                    self.zoom = Zoom::Percent(p);
                }
                Task::none()
            }
            ImageViewerMessage::ZoomOut => {
                let ratio = self.view_ratio.get();
                if let Some(p) = Zoom::percents()
                    .filter(|&p| p as f32 / 100.0 < ratio)
                    .last()
                {
                    self.zoom = Zoom::Percent(p);
                }
                Task::none()
            }
        }
    }

    fn view_context(&self) -> ContextView<'_, ImageViewerMessage> {
        let zoom_section = widget::settings::section()
            .title(fl!("epub-viewer-image-zoom"))
            .add(
                widget::settings::item::builder(fl!("epub-viewer-image-zoom")).control(
                    widget::dropdown(
                        &self.zoom_names,
                        Zoom::all().iter().position(|z| z == &self.zoom),
                        ImageViewerMessage::ZoomDropdown,
                    ),
                ),
            );

        ContextView {
            title: self.display_name(),
            content: zoom_section.into(),
        }
    }
}
