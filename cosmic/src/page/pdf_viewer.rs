use std::cell::Cell;
use std::fmt;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use cosmic::Action;
use cosmic::Element;
use cosmic::Task;
use cosmic::cosmic_theme;
use cosmic::iced::ContentFit;
use cosmic::iced::Length;
use cosmic::iced::Rectangle;
use cosmic::iced::alignment::Vertical;
use cosmic::iced::core::SmolStr;
use cosmic::iced::keyboard::Key;
use cosmic::iced::keyboard::Modifiers;
use cosmic::iced::keyboard::key::Named;
use cosmic::iced::mouse::ScrollDelta;
use cosmic::iced::widget::scrollable;
use cosmic::theme;
use cosmic::widget;

use crate::ICON_SIZE;
use crate::aggregator::Document;
use crate::app::ContextView;
use crate::client::ClientSelector;
use crate::fl;

type Fingerprint = String;

const THUMBNAIL_WIDTH: u16 = 128;

// --- Core types extracted from cosmic-reader ---

#[derive(Clone, Debug)]
pub(crate) struct PdfPage {
    index: i32,
    bounds: mupdf::Rect,
    display_list: Option<Arc<mupdf::DisplayList>>,
    icon_bounds: Cell<Option<Rectangle>>,
    icon_handle: Option<widget::image::Handle>,
    svg_handle: Option<widget::svg::Handle>,
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
            Zoom::Percent(175),
            Zoom::Percent(200),
            Zoom::Percent(225),
            Zoom::Percent(250),
            Zoom::Percent(275),
            Zoom::Percent(300),
            Zoom::Percent(325),
            Zoom::Percent(350),
            Zoom::Percent(375),
            Zoom::Percent(400),
            Zoom::Percent(425),
            Zoom::Percent(450),
            Zoom::Percent(475),
            Zoom::Percent(500),
        ]
    }
}

impl fmt::Display for Zoom {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Zoom::FitBoth => write!(f, "Fit width and height"),
            Zoom::FitHeight => write!(f, "Fit height"),
            Zoom::FitWidth => write!(f, "Fit width"),
            Zoom::Percent(percent) => write!(f, "{}%", percent),
        }
    }
}

fn display_list_to_image(display_list: &mupdf::DisplayList, scale: f32) -> widget::image::Handle {
    let matrix = mupdf::Matrix::new_scale(scale, scale);
    let pixmap = display_list
        .to_pixmap(&matrix, &mupdf::Colorspace::device_rgb(), false)
        .unwrap();
    let mut data = Vec::new();
    pixmap.write_to(&mut data, mupdf::ImageFormat::PNG).unwrap();
    widget::image::Handle::from_bytes(data)
}

// --- Messages ---

#[derive(Debug, Clone)]
pub enum PdfViewerOutput {
    Close(Fingerprint),
}

#[derive(Clone, Debug)]
pub enum PdfViewerMessage {
    // PDF loading pipeline
    PagesLoaded(Vec<PdfPage>),
    DisplayListReady(i32, Arc<mupdf::DisplayList>),
    ThumbnailReady(i32, widget::image::Handle),
    SvgReady(i32, widget::svg::Handle),

    // Navigation
    SelectPage(usize),
    ThumbnailScroll(scrollable::Viewport),

    // Zoom
    ZoomDropdown(usize),
    ZoomScroll(ScrollDelta),

    // Search
    SearchActivate,
    SearchClear,
    SearchInput(String),

    // Keyboard / input
    Key(Modifiers, Key, Option<SmolStr>),
    ModifiersChanged(Modifiers),

    // Outgoing
    Out(PdfViewerOutput),
}

// --- PdfViewer page ---

pub struct PdfViewer {
    fingerprint: Fingerprint,
    document: Document,
    file_path: Option<PathBuf>,

    // PDF state
    pages: Vec<PdfPage>,
    active_page: usize,
    zoom: Zoom,
    zoom_names: Vec<String>,
    search_active: bool,
    search_id: widget::Id,
    search_term: String,
    modifiers: Modifiers,
    view_ratio: Cell<f32>,
    zoom_scroll: f32,

    // Thumbnail panel state
    thumbnail_scroll_id: widget::Id,
    thumbnail_viewport: Option<scrollable::Viewport>,
}

impl PdfViewer {
    pub fn new(document: Document) -> (Self, Task<Action<PdfViewerMessage>>) {
        let fingerprint = document.metadata.fingerprint.clone();

        // Resolve local file path from document sources
        let sources = document.sources_by_priority();
        let local_source = sources.iter().find(|s| s.client == ClientSelector::Local);
        let file_path = local_source.map(|s| PathBuf::from(&s.path));

        let zoom_names: Vec<String> = Zoom::all().iter().map(|z| z.to_string()).collect();

        let viewer = PdfViewer {
            fingerprint,
            document,
            file_path: file_path.clone(),
            pages: Vec::new(),
            active_page: 0,
            zoom: Zoom::FitBoth,
            zoom_names,
            search_active: false,
            search_id: widget::Id::unique(),
            search_term: String::new(),
            modifiers: Modifiers::default(),
            view_ratio: Cell::new(1.0),
            zoom_scroll: 0.0,
            thumbnail_scroll_id: widget::Id::unique(),
            thumbnail_viewport: None,
        };

        // Start loading the PDF if we have a local path
        let task = if let Some(path) = file_path {
            Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || load_pdf_pages(&path))
                        .await
                        .unwrap()
                },
                |pages| cosmic::action::app(PdfViewerMessage::PagesLoaded(pages)),
            )
        } else {
            Task::none()
        };

        (viewer, task)
    }

    pub fn display_name(&self) -> String {
        Path::new(&self.document.sources.iter().next().unwrap().path)
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("PDF")
            .to_string()
    }

    pub fn view(&self) -> Element<'_, PdfViewerMessage> {
        let cosmic_theme::Spacing {
            space_xxs,
            space_xs,
            space_s,
            ..
        } = theme::active().cosmic().spacing;

        if self.pages.is_empty() {
            // Loading state
            return widget::column()
                .push(self.view_header())
                .push(
                    widget::container(widget::text::body(fl!("pdf-viewer-loading")))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .align_x(cosmic::iced::alignment::Horizontal::Center)
                        .align_y(Vertical::Center),
                )
                .into();
        }

        let header = self.view_header();
        let thumbnails = self.view_thumbnails();
        let content = self.view_content();

        let body = widget::row()
            .push(thumbnails)
            .push(content)
            .height(Length::Fill);

        widget::column()
            .push(header)
            .push(body)
            .height(Length::Fill)
            .into()
    }

    pub fn view_context(&self) -> ContextView<'_, PdfViewerMessage> {
        ContextView {
            title: self.display_name(),
            content: widget::horizontal_space().into(),
        }
    }

    fn view_header(&self) -> Element<'_, PdfViewerMessage> {
        let cosmic_theme::Spacing {
            space_xxs,
            space_xs,
            space_s,
            ..
        } = theme::active().cosmic().spacing;

        let path = Path::new(&self.document.sources.iter().next().unwrap().path);
        let filename = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("PDF");

        let page_info = if !self.pages.is_empty() {
            format!("{} / {}", self.active_page + 1, self.pages.len())
        } else {
            String::new()
        };

        let mut row = widget::row()
            .spacing(space_s)
            .align_y(Vertical::Center)
            .push(
                widget::button::icon(
                    widget::icon::from_name("go-previous-symbolic").size(ICON_SIZE),
                )
                .on_press(PdfViewerMessage::Out(PdfViewerOutput::Close(
                    self.fingerprint.clone(),
                )))
                .tooltip(fl!("pdf-viewer-back")),
            )
            .push(
                widget::icon::from_name("application-pdf-symbolic")
                    .size(24)
                    .icon(),
            )
            .push(widget::text::heading(filename).width(Length::Fill))
            .push(widget::text::body(page_info));

        // Search
        if self.search_active {
            row = row.push(
                widget::text_input::search_input("", &self.search_term)
                    .width(Length::Fixed(240.0))
                    .id(self.search_id.clone())
                    .on_clear(PdfViewerMessage::SearchClear)
                    .on_input(PdfViewerMessage::SearchInput),
            );
        } else {
            row = row.push(
                widget::button::icon(
                    widget::icon::from_name("system-search-symbolic").size(ICON_SIZE),
                )
                .on_press(PdfViewerMessage::SearchActivate)
                .padding(space_xxs),
            );
        }

        // Zoom dropdown
        row = row.push(widget::dropdown(
            &self.zoom_names,
            Zoom::all().iter().position(|z| z == &self.zoom),
            PdfViewerMessage::ZoomDropdown,
        ));

        widget::container(row).padding(space_xs).into()
    }

    fn view_thumbnails(&self) -> Element<'_, PdfViewerMessage> {
        let cosmic_theme::Spacing { space_xxs, .. } = theme::active().cosmic().spacing;

        let mut column = widget::column::with_capacity(self.pages.len())
            .padding(space_xxs)
            .spacing(space_xxs);

        let x = space_xxs as f32;
        let mut y = space_xxs as f32;
        let mut count = 0;

        for (idx, page) in self.pages.iter().enumerate() {
            if count > 0 {
                y += space_xxs as f32;
            }

            let width = THUMBNAIL_WIDTH as f32;
            let height = page.bounds.height() * width / page.bounds.width();
            page.icon_bounds.set(Some(Rectangle {
                x,
                y,
                width,
                height,
            }));

            if let Some(handle) = &page.icon_handle {
                column = column.push(
                    widget::button::image(handle)
                        .width(width)
                        .height(height)
                        .on_press(PdfViewerMessage::SelectPage(idx))
                        .selected(idx == self.active_page),
                );
            } else {
                column = column.push(
                    widget::button::custom_image_button(
                        widget::Space::with_height(Length::Fixed(height)),
                        None,
                    )
                    .width(width)
                    .height(height)
                    .on_press(PdfViewerMessage::SelectPage(idx))
                    .selected(idx == self.active_page),
                );
            }

            y += height;
            count += 1;
        }

        widget::container(
            widget::scrollable(column)
                .id(self.thumbnail_scroll_id.clone())
                .on_scroll(|v| PdfViewerMessage::ThumbnailScroll(v))
                .width(Length::Fixed(
                    (THUMBNAIL_WIDTH as f32) + (space_xxs as f32) * 2.0,
                )),
        )
        .height(Length::Fill)
        .into()
    }

    fn view_content(&self) -> Element<'_, PdfViewerMessage> {
        if let Some(page) = self.pages.get(self.active_page) {
            widget::responsive(move |size| {
                let ratio = match self.zoom {
                    Zoom::FitHeight => size.height / page.bounds.height(),
                    Zoom::FitWidth => size.width / page.bounds.width(),
                    Zoom::FitBoth => {
                        (size.width / page.bounds.width()).min(size.height / page.bounds.height())
                    }
                    Zoom::Percent(percent) => (percent as f32) / 100.0,
                };
                self.view_ratio.set(ratio);

                let width = page.bounds.width() * ratio;
                let height = page.bounds.height() * ratio;

                let mut container = widget::container(if let Some(handle) = &page.svg_handle {
                    Element::from(
                        widget::svg(handle.clone())
                            .content_fit(ContentFit::Fill)
                            .width(width)
                            .height(height),
                    )
                } else {
                    Element::from(widget::Space::new(width, height))
                })
                .style(|_theme| widget::container::background(cosmic::iced::Color::WHITE));

                if size.width > width {
                    container = container.center_x(size.width);
                }
                if size.height > height {
                    container = container.center_y(size.height);
                }

                let mut mouse_area = widget::mouse_area(container);
                if self.modifiers.contains(Modifiers::CTRL) {
                    mouse_area = mouse_area.on_scroll(PdfViewerMessage::ZoomScroll);
                }

                widget::scrollable(mouse_area)
                    .direction(scrollable::Direction::Both {
                        vertical: Default::default(),
                        horizontal: Default::default(),
                    })
                    .into()
            })
            .into()
        } else {
            widget::Space::new(Length::Fill, Length::Fill).into()
        }
    }

    fn update_active_page(&mut self) -> Task<Action<PdfViewerMessage>> {
        let mut tasks = Vec::with_capacity(2);

        // Auto-scroll thumbnails to keep active page visible
        if let Some(viewport) = &self.thumbnail_viewport {
            if let Some(page) = self.pages.get(self.active_page) {
                let mut bounds = viewport.bounds();
                let offset = viewport.absolute_offset();
                bounds.x = offset.x;
                bounds.y = offset.y;

                if let Some(icon_bounds) = page.icon_bounds.get() {
                    if bounds.y > icon_bounds.y {
                        tasks.push(scrollable::scroll_to(
                            self.thumbnail_scroll_id.clone(),
                            scrollable::AbsoluteOffset {
                                x: 0.0,
                                y: icon_bounds.y,
                            },
                        ));
                    } else if bounds.y + bounds.height < icon_bounds.y + icon_bounds.height {
                        tasks.push(scrollable::scroll_to(
                            self.thumbnail_scroll_id.clone(),
                            scrollable::AbsoluteOffset {
                                x: 0.0,
                                y: icon_bounds.y + icon_bounds.height - bounds.height,
                            },
                        ));
                    }
                }
            }
        }

        // Generate SVG for active page if not already available
        if let Some(page) = self.pages.get(self.active_page) {
            if page.svg_handle.is_none() {
                if let Some(display_list) = page.display_list.clone() {
                    let index = page.index;
                    tasks.push(Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                let svg = display_list.to_svg(&mupdf::Matrix::IDENTITY).unwrap();
                                PdfViewerMessage::SvgReady(
                                    index,
                                    widget::svg::Handle::from_memory(svg.into_bytes()),
                                )
                            })
                            .await
                            .unwrap()
                        },
                        cosmic::action::app,
                    ));
                }
            }
        }

        Task::batch(tasks)
    }

    fn page_index_by_pdf_index(&self, pdf_index: i32) -> Option<usize> {
        self.pages.iter().position(|p| p.index == pdf_index)
    }

    pub fn update(&mut self, message: PdfViewerMessage) -> Task<Action<PdfViewerMessage>> {
        match message {
            PdfViewerMessage::PagesLoaded(pages) => {
                self.pages = pages;
                if !self.pages.is_empty() {
                    self.active_page = 0;
                    // Start generating display lists for all pages
                    let tasks: Vec<_> = self
                        .pages
                        .iter()
                        .map(|page| {
                            let path = self.file_path.clone().unwrap();
                            let index = page.index;
                            Task::perform(
                                async move {
                                    tokio::task::spawn_blocking(move || {
                                        let doc = mupdf::Document::open(path.as_os_str()).unwrap();
                                        let page = doc.load_page(index).unwrap();
                                        let display_list = page.to_display_list(false).unwrap();
                                        PdfViewerMessage::DisplayListReady(
                                            index,
                                            Arc::new(display_list),
                                        )
                                    })
                                    .await
                                    .unwrap()
                                },
                                cosmic::action::app,
                            )
                        })
                        .collect();
                    return Task::batch(tasks).chain(self.update_active_page());
                }
                Task::none()
            }
            PdfViewerMessage::DisplayListReady(pdf_index, display_list) => {
                if let Some(idx) = self.page_index_by_pdf_index(pdf_index) {
                    self.pages[idx].display_list = Some(display_list.clone());

                    let mut tasks = Vec::with_capacity(2);

                    // Generate thumbnail
                    tasks.push(Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                let scale =
                                    (THUMBNAIL_WIDTH as f32) / display_list.bounds().width();
                                PdfViewerMessage::ThumbnailReady(
                                    pdf_index,
                                    display_list_to_image(&display_list, scale),
                                )
                            })
                            .await
                            .unwrap()
                        },
                        cosmic::action::app,
                    ));

                    // If this is the active page, trigger SVG generation
                    if idx == self.active_page {
                        tasks.push(self.update_active_page());
                    }

                    return Task::batch(tasks);
                }
                Task::none()
            }
            PdfViewerMessage::ThumbnailReady(pdf_index, handle) => {
                if let Some(idx) = self.page_index_by_pdf_index(pdf_index) {
                    self.pages[idx].icon_handle = Some(handle);
                }
                Task::none()
            }
            PdfViewerMessage::SvgReady(pdf_index, handle) => {
                if let Some(idx) = self.page_index_by_pdf_index(pdf_index) {
                    self.pages[idx].svg_handle = Some(handle);
                }
                Task::none()
            }
            PdfViewerMessage::SelectPage(idx) => {
                if idx < self.pages.len() {
                    self.active_page = idx;
                    return self.update_active_page();
                }
                Task::none()
            }
            PdfViewerMessage::ThumbnailScroll(viewport) => {
                self.thumbnail_viewport = Some(viewport);
                Task::none()
            }
            PdfViewerMessage::ZoomDropdown(index) => {
                if let Some(zoom) = Zoom::all().get(index) {
                    self.zoom = *zoom;
                }
                Task::none()
            }
            PdfViewerMessage::ZoomScroll(delta) => {
                self.zoom_scroll += match delta {
                    ScrollDelta::Lines { y, .. } => y,
                    ScrollDelta::Pixels { y, .. } => y / 20.0,
                };
                let mut percent = match self.zoom {
                    Zoom::Percent(percent) => percent,
                    _ => ((self.view_ratio.get() * 4.0).round() as i16) * 25,
                };
                while self.zoom_scroll >= 1.0 {
                    percent += 25;
                    self.zoom_scroll -= 1.0;
                }
                while self.zoom_scroll <= -1.0 {
                    percent -= 25;
                    self.zoom_scroll += 1.0;
                }
                self.zoom = Zoom::Percent(percent.clamp(25, 500));
                Task::none()
            }
            PdfViewerMessage::SearchActivate => {
                self.search_active = true;
                widget::text_input::focus(self.search_id.clone())
            }
            PdfViewerMessage::SearchClear => {
                self.search_active = false;
                self.search_term.clear();
                Task::none()
            }
            PdfViewerMessage::SearchInput(term) => {
                self.search_term = term;
                Task::none()
            }
            PdfViewerMessage::Key(_modifiers, key, _text) => match &key {
                Key::Named(Named::ArrowUp | Named::ArrowLeft | Named::PageUp) => {
                    if self.active_page > 0 {
                        self.active_page -= 1;
                    }
                    self.update_active_page()
                }
                Key::Named(Named::ArrowDown | Named::ArrowRight | Named::PageDown) => {
                    if self.active_page + 1 < self.pages.len() {
                        self.active_page += 1;
                    }
                    self.update_active_page()
                }
                Key::Named(Named::Escape) => {
                    self.search_active = false;
                    self.search_term.clear();
                    Task::none()
                }
                Key::Character(c) => match c.as_str() {
                    "0" => {
                        self.zoom = Zoom::Percent(100);
                        Task::none()
                    }
                    "-" => {
                        let percent = match self.zoom {
                            Zoom::Percent(percent) => percent,
                            _ => ((self.view_ratio.get() * 4.0).round() as i16) * 25,
                        };
                        self.zoom = Zoom::Percent((percent - 25).clamp(25, 500));
                        Task::none()
                    }
                    "=" => {
                        let percent = match self.zoom {
                            Zoom::Percent(percent) => percent,
                            _ => ((self.view_ratio.get() * 4.0).round() as i16) * 25,
                        };
                        self.zoom = Zoom::Percent((percent + 25).clamp(25, 500));
                        Task::none()
                    }
                    "f" => {
                        self.zoom = Zoom::FitBoth;
                        Task::none()
                    }
                    "h" => {
                        self.zoom = Zoom::FitHeight;
                        Task::none()
                    }
                    "w" => {
                        self.zoom = Zoom::FitWidth;
                        Task::none()
                    }
                    "s" | "/" => {
                        self.search_active = true;
                        widget::text_input::focus(self.search_id.clone())
                    }
                    _ => Task::none(),
                },
                _ => Task::none(),
            },
            PdfViewerMessage::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers;
                Task::none()
            }
            PdfViewerMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent component")
            }
        }
    }
}

/// Load PDF pages (bounds only) from a file path. Runs on a blocking thread.
fn load_pdf_pages(path: &Path) -> Vec<PdfPage> {
    let doc = mupdf::Document::open(path.as_os_str()).unwrap();
    let page_count = doc.page_count().unwrap();

    let mut pages = Vec::with_capacity(usize::try_from(page_count).unwrap());
    for index in 0..page_count {
        let page = doc.load_page(index).unwrap();
        let bounds = page.bounds().unwrap();
        pages.push(PdfPage {
            index,
            bounds,
            display_list: None,
            icon_bounds: Cell::new(None),
            icon_handle: None,
            svg_handle: None,
        });
    }
    pages
}
