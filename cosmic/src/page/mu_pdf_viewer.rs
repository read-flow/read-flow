use std::cell::Cell;
use std::fmt;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use cosmic::Action;
use cosmic::Application;
use cosmic::Apply;
use cosmic::Element;
use cosmic::Task;
use cosmic::cosmic_config;
use cosmic::cosmic_config::ConfigGet;
use cosmic::cosmic_config::ConfigSet;
use cosmic::cosmic_theme;
use cosmic::iced::ContentFit;
use cosmic::iced::Length;
use cosmic::iced::Rectangle;
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::alignment::Vertical;
use cosmic::iced::core::SmolStr;
use cosmic::iced::keyboard::Key;
use cosmic::iced::keyboard::Modifiers;
use cosmic::iced::keyboard::key::Named;
use cosmic::iced::mouse::ScrollDelta;
use cosmic::iced::widget::scrollable;
use cosmic::theme;
use cosmic::widget;
use read_flow_core::Builder;

use crate::ICON_SIZE;
use crate::aggregator::Document;
use crate::app::ContextView;
use crate::app::ReadFlow;
use crate::client::ClientSelector;
use crate::document_provider::DocumentProvider;
use crate::fl;
use crate::page::Page;

type Fingerprint = String;

const THUMBNAIL_WIDTH: u16 = 128;
/// Minimum total viewer width below which the thumbnail pane is hidden.
const MIN_WIDTH_WITH_THUMBNAILS: f32 = 500.0;

const MUPDF_PREFS_VERSION: u64 = 1;
const KEY_EPUB_FONT_SIZE: &str = "mupdf_epub_font_size";

fn load_mupdf_prefs() -> f32 {
    let Ok(ctx) = cosmic_config::Config::new(ReadFlow::APP_ID, MUPDF_PREFS_VERSION) else {
        return 12.0;
    };
    let size_pt: u32 = ctx.get(KEY_EPUB_FONT_SIZE).unwrap_or(12);
    (size_pt as f32).clamp(8.0, 24.0)
}

fn save_mupdf_epub_font_size(size: f32) {
    let Ok(ctx) = cosmic_config::Config::new(ReadFlow::APP_ID, MUPDF_PREFS_VERSION) else {
        return;
    };
    let _ = ctx.set(KEY_EPUB_FONT_SIZE, size.round() as u32);
}

enum DiscoveryItem {
    Page(PdfPage),
    Done(bool),
    DisplayList(i32, Arc<mupdf::DisplayList>),
    Error(String),
}

/// Whether to display two pages side by side.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DualPageMode {
    #[default]
    Auto,
    Off,
    On,
}

// --- Core types extracted from cosmic-reader ---

#[derive(Clone, Debug)]
pub(crate) struct PdfPage {
    index: i32,
    bounds: mupdf::Rect,
    display_list: Option<Arc<mupdf::DisplayList>>,
    icon_bounds: Cell<Option<Rectangle>>,
    icon_handle: Option<widget::image::Handle>,
    svg_handle: Option<widget::svg::Handle>,
    raster_handle: Option<widget::image::Handle>,
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

fn display_list_to_image_tinted(
    display_list: &mupdf::DisplayList,
    scale: f32,
    text_color: (u8, u8, u8),
    bg_color: (u8, u8, u8),
) -> widget::image::Handle {
    let matrix = mupdf::Matrix::new_scale(scale, scale);
    let mut pixmap = display_list
        .to_pixmap(&matrix, &mupdf::Colorspace::device_rgb(), false)
        .unwrap();
    let black =
        ((text_color.0 as i32) << 16) | ((text_color.1 as i32) << 8) | (text_color.2 as i32);
    let white = ((bg_color.0 as i32) << 16) | ((bg_color.1 as i32) << 8) | (bg_color.2 as i32);
    pixmap.tint(black, white).unwrap();
    let rgb = pixmap.samples();
    let mut rgba: Vec<u8> = Vec::with_capacity(rgb.len() / 3 * 4);
    rgba.extend(rgb.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255u8]));
    widget::image::Handle::from_rgba(pixmap.width(), pixmap.height(), rgba)
}

// --- Messages ---

#[derive(Clone, Debug)]
pub enum MuPdfViewerOutput {
    /// (fingerprint, page, total_pages) — None when pages not yet loaded.
    Close(Fingerprint, Option<(usize, usize)>),
    OpenDocumentDetails(Box<Document>),
}

#[derive(Clone, Debug)]
pub enum MuPdfViewerMessage {
    // PDF loading pipeline
    ReadingProgressLoaded(Option<usize>),
    PageDiscovered(u64, PdfPage),
    PagesDiscoveryComplete(u64, bool),
    DisplayListReady(u64, i32, Arc<mupdf::DisplayList>),
    ThumbnailReady(u64, i32, widget::image::Handle),
    SvgReady(u64, i32, widget::svg::Handle),
    RasterReady(u64, i32, widget::image::Handle),

    // Navigation
    SelectPage(usize),
    PreviousPage,
    NextPage,
    ThumbnailScroll(scrollable::Viewport),

    // Zoom
    ZoomDropdown(usize),
    ZoomScroll(ScrollDelta),

    // UI settings
    ThemeColors(bool),
    ShowThumbnails(bool),
    DualPane(DualPageMode),
    EpubFontSize(f32),

    // Keyboard / input
    Key(Modifiers, Key, Option<SmolStr>),
    ModifiersChanged(Modifiers),

    // Error
    LoadFailed(String),

    // Outgoing
    Out(MuPdfViewerOutput),
}

// --- MuPdfViewer page ---

/// @feature: reading.pdf_viewer
pub struct MuPdfViewer {
    fingerprint: Fingerprint,
    document: Document,
    file_path: Option<PathBuf>,
    load_error: Option<String>,

    // PDF state
    pages: Vec<PdfPage>,
    active_page: usize,
    initial_page: Option<usize>,
    zoom: Zoom,
    zoom_names: Vec<String>,
    modifiers: Modifiers,
    view_ratio: Cell<f32>,
    zoom_scroll: f32,
    theme_colors: bool,
    show_thumbnails: bool,
    dual_pane: DualPageMode,
    is_reflowable: bool,
    epub_font_size: f32,
    layout_gen: u64,
    /// Most recently observed content viewport dimensions, set from the
    /// `responsive` closure in `view_content` (via Cell, since `view()` takes `&self`).
    viewport_size: Cell<(f32, f32)>,

    // Thumbnail panel state
    thumbnail_scroll_id: widget::Id,
    thumbnail_viewport: Option<scrollable::Viewport>,
}

impl MuPdfViewer {
    pub fn new(
        document: Document,
        document_provider: Arc<DocumentProvider>,
    ) -> (Self, Task<Action<MuPdfViewerMessage>>) {
        let fingerprint = document
            .contents
            .first()
            .map(|c| c.fingerprint.clone())
            .unwrap_or_default();

        // Resolve local file path from document sources
        let sources = document.sources_by_priority();
        let local_source = sources
            .iter()
            .find(|(_, s)| s.client == ClientSelector::Local);
        let file_path = local_source.map(|(_, s)| PathBuf::from(&s.path));

        let zoom_names: Vec<String> = Zoom::all().iter().map(|z| z.to_string()).collect();

        let viewer = MuPdfViewer {
            fingerprint: fingerprint.clone(),
            document,
            file_path: file_path.clone(),
            load_error: None,
            pages: Vec::new(),
            active_page: 0,
            initial_page: None,
            zoom: Zoom::FitBoth,
            zoom_names,
            modifiers: Modifiers::default(),
            view_ratio: Cell::new(1.0),
            zoom_scroll: 0.0,
            theme_colors: false,
            show_thumbnails: true,
            dual_pane: DualPageMode::default(),
            is_reflowable: false,
            epub_font_size: load_mupdf_prefs(),
            layout_gen: 0,
            viewport_size: Cell::new((0.0, 0.0)),
            thumbnail_scroll_id: widget::Id::unique(),
            thumbnail_viewport: None,
        };

        let mut tasks = Vec::new();

        // Start loading the PDF if we have a local path, streaming one page at a time
        tasks.push(viewer.start_discovery());

        // Fetch reading progress
        let fp = fingerprint;
        tasks.push(Task::perform(
            async move {
                let aggregator = document_provider.aggregator.read().await;
                match aggregator.get_reading_state(&fp).await {
                    Ok(Some(state)) => parse_page_from_progress(&state.position),
                    Ok(None) => None,
                    Err(e) => {
                        tracing::warn!("failed to load reading state: {e}");
                        None
                    }
                }
            },
            |page| cosmic::action::app(MuPdfViewerMessage::ReadingProgressLoaded(page)),
        ));

        (viewer, Task::batch(tasks))
    }

    fn start_discovery(&self) -> Task<Action<MuPdfViewerMessage>> {
        let Some(path) = self.file_path.clone() else {
            return Task::none();
        };
        use futures::StreamExt as _;
        let (tx, rx) = futures::channel::mpsc::unbounded::<DiscoveryItem>();
        let layout_gen = self.layout_gen;
        let em = self.epub_font_size;
        tokio::task::spawn_blocking(move || {
            let mut doc = match mupdf::Document::open(path.as_os_str()) {
                Ok(d) => d,
                Err(e) => {
                    let _ = tx.unbounded_send(DiscoveryItem::Error(e.to_string()));
                    return;
                }
            };
            let is_reflowable = doc.is_reflowable().unwrap_or(false);
            if is_reflowable {
                let _ = doc.layout(595.0, 842.0, em);
            }
            let count = doc.page_count().unwrap();
            // First pass: send page bounds so the UI can show the loading state
            for index in 0..count {
                let page = doc.load_page(index).unwrap();
                let bounds = page.bounds().unwrap();
                let _ = tx.unbounded_send(DiscoveryItem::Page(PdfPage {
                    index,
                    bounds,
                    display_list: None,
                    icon_bounds: Cell::new(None),
                    icon_handle: None,
                    svg_handle: None,
                    raster_handle: None,
                }));
            }
            let _ = tx.unbounded_send(DiscoveryItem::Done(is_reflowable));
            // Second pass: generate display lists while the document is still open,
            // avoiding the cost of re-opening the file once per page.
            for index in 0..count {
                let page = doc.load_page(index).unwrap();
                let display_list = page.to_display_list(false).unwrap();
                let _ =
                    tx.unbounded_send(DiscoveryItem::DisplayList(index, Arc::new(display_list)));
            }
        });
        Task::run(
            rx.map(move |item| match item {
                DiscoveryItem::Page(p) => MuPdfViewerMessage::PageDiscovered(layout_gen, p),
                DiscoveryItem::Done(r) => MuPdfViewerMessage::PagesDiscoveryComplete(layout_gen, r),
                DiscoveryItem::DisplayList(index, dl) => {
                    MuPdfViewerMessage::DisplayListReady(layout_gen, index, dl)
                }
                DiscoveryItem::Error(e) => MuPdfViewerMessage::LoadFailed(e),
            }),
            cosmic::action::app,
        )
    }

    fn repaginate(&mut self) -> Task<Action<MuPdfViewerMessage>> {
        self.layout_gen += 1;
        self.pages.clear();
        self.active_page = 0;
        self.start_discovery()
    }

    pub fn display_name(&self) -> String {
        let path = self
            .document
            .contents
            .first()
            .and_then(|c| c.sources.first())
            .map(|s| s.path.as_str())
            .unwrap_or("PDF");
        Path::new(path)
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("PDF")
            .to_string()
    }

    fn view_thumbnails(&self) -> Element<'_, MuPdfViewerMessage> {
        let cosmic_theme::Spacing { space_xxs, .. } = theme::active().cosmic().spacing;

        let mut column = widget::column::with_capacity(self.pages.len())
            .padding(space_xxs)
            .spacing(space_xxs);

        let x = space_xxs as f32;
        let mut y = space_xxs as f32;
        let visible = self.visible_page_indices();

        for (idx, page) in self.pages.iter().enumerate() {
            if idx > 0 {
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
                        .on_press(MuPdfViewerMessage::SelectPage(idx))
                        .selected(visible.contains(&idx)),
                );
            } else {
                column = column.push(
                    widget::button::custom_image_button(
                        widget::Space::new().height(Length::Fixed(height)),
                        None,
                    )
                    .width(width)
                    .height(height)
                    .on_press(MuPdfViewerMessage::SelectPage(idx))
                    .selected(visible.contains(&idx)),
                );
            }

            y += height;
        }

        let page_info = if !self.pages.is_empty() {
            format!("{} / {}", self.active_page + 1, self.pages.len())
        } else {
            String::new()
        };

        let toggle_btn =
            widget::button::icon(widget::icon::from_name("navbar-open-symbolic").size(ICON_SIZE))
                .on_press(MuPdfViewerMessage::ShowThumbnails(false));

        widget::Column::with_children(vec![
            widget::Row::new()
                .push(toggle_btn)
                .push(
                    widget::Column::with_children(vec![
                        widget::text::body(page_info)
                            .wrapping(cosmic::iced::widget::text::Wrapping::None)
                            .into(),
                    ])
                    .width(Length::Fill)
                    .align_x(Horizontal::Center),
                )
                .align_y(cosmic::iced::Alignment::Center)
                .width(Length::Fill)
                .into(),
            widget::scrollable(column)
                .id(self.thumbnail_scroll_id.clone())
                .on_scroll(MuPdfViewerMessage::ThumbnailScroll)
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
        ])
        .width(Length::Fixed(
            (THUMBNAIL_WIDTH as f32) + (space_xxs as f32) * 2.0,
        ))
        .height(Length::Fill)
        .into()
    }

    fn view_content(&self) -> Element<'_, MuPdfViewerMessage> {
        widget::responsive(move |size| {
            self.viewport_size.set((size.width, size.height));
            let dual = self.should_dual_pane(size.width);
            let content: Element<'_, MuPdfViewerMessage> = if dual {
                let cosmic_theme::Spacing { space_xxs, .. } = theme::active().cosmic().spacing;
                let first_page = (self.active_page / 2) * 2;
                vec![
                    self.view_pdf_page(first_page, Horizontal::Right),
                    self.view_pdf_page(first_page + 1, Horizontal::Left),
                ]
                .apply(widget::Row::with_children)
                .spacing(space_xxs)
                .into()
            } else {
                self.view_pdf_page(self.active_page, Horizontal::Center)
            };

            let left_zone = widget::mouse_area(
                widget::container(widget::icon::from_name("go-previous-symbolic").size(ICON_SIZE))
                    .width(Length::FillPortion(1))
                    .height(Length::Fill)
                    .center(Length::Fill),
            )
            .on_press(MuPdfViewerMessage::PreviousPage);

            let center = widget::container(content)
                .width(Length::FillPortion(10))
                .height(Length::Fill);

            let right_zone = widget::mouse_area(
                widget::container(widget::icon::from_name("go-next-symbolic").size(ICON_SIZE))
                    .width(Length::FillPortion(1))
                    .height(Length::Fill)
                    .center(Length::Fill),
            )
            .on_press(MuPdfViewerMessage::NextPage);

            widget::Row::new()
                .push(left_zone)
                .push(center)
                .push(right_zone)
                .height(Length::Fill)
                .into()
        })
        .into()
    }

    /// Whether dual-page display should be active at the given viewport width.
    fn should_dual_pane(&self, viewport_width: f32) -> bool {
        match self.dual_pane {
            DualPageMode::On => true,
            DualPageMode::Off => false,
            DualPageMode::Auto => viewport_width > 1200.0,
        }
    }

    fn view_pdf_page(
        &self,
        page_idx: usize,
        align_x: Horizontal,
    ) -> Element<'_, MuPdfViewerMessage> {
        if let Some(page) = self.pages.get(page_idx) {
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

                // Inner container: white "paper" background for the PDF page
                let paper = widget::container(if let Some(handle) = &page.raster_handle {
                    Element::from(
                        widget::image(handle.clone())
                            .content_fit(ContentFit::Fill)
                            .width(width)
                            .height(height),
                    )
                } else if let Some(handle) = &page.svg_handle {
                    Element::from(
                        widget::svg(handle.clone())
                            .content_fit(ContentFit::Fill)
                            .width(width)
                            .height(height),
                    )
                } else {
                    Element::from(widget::Space::new().width(width).height(height))
                })
                .style(if self.theme_colors {
                    |theme: &cosmic::Theme| {
                        let c = theme.cosmic().bg_color();
                        widget::container::background(cosmic::iced::Color::from_rgba(
                            c.color.red,
                            c.color.green,
                            c.color.blue,
                            c.alpha,
                        ))
                    }
                } else {
                    |_theme: &cosmic::Theme| {
                        widget::container::background(cosmic::iced::Color::WHITE)
                    }
                });

                // Outer container: theme background surrounding the paper
                let outer = widget::container(paper)
                    .apply_if(size.width > width, |outer| {
                        outer.align_x(align_x).width(size.width)
                    })
                    .apply_if(size.height > height, |outer| outer.center_y(size.height));

                let mouse_area = widget::mouse_area(outer)
                    .apply_if(self.modifiers.contains(Modifiers::CTRL), |mouse_area| {
                        mouse_area.on_scroll(MuPdfViewerMessage::ZoomScroll)
                    });

                widget::scrollable(mouse_area)
                    .direction(scrollable::Direction::Both {
                        vertical: Default::default(),
                        horizontal: Default::default(),
                    })
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            })
            .into()
        } else {
            widget::Space::new()
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        }
    }

    fn update_active_page(&mut self) -> Task<Action<MuPdfViewerMessage>> {
        let mut tasks = Vec::with_capacity(2);

        // Auto-scroll thumbnails to keep active page visible
        if let Some(viewport) = &self.thumbnail_viewport
            && let Some(page) = self.pages.get(self.active_page)
        {
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
                        }
                        .into(),
                    ));
                } else if bounds.y + bounds.height < icon_bounds.y + icon_bounds.height {
                    tasks.push(scrollable::scroll_to(
                        self.thumbnail_scroll_id.clone(),
                        scrollable::AbsoluteOffset {
                            x: 0.0,
                            y: icon_bounds.y + icon_bounds.height - bounds.height,
                        }
                        .into(),
                    ));
                }
            }
        }

        // Render visible pages. For the raster path also pre-render the next
        // two pages so they are ready before the user navigates to them,
        // avoiding a blank frame while waiting for slow raster generation.
        let mut pages_to_render = self.visible_page_indices();
        if self.theme_colors {
            for offset in 1..=2usize {
                if let Some(i) = self
                    .active_page
                    .checked_add(offset)
                    .filter(|&i| i < self.pages.len())
                    && !pages_to_render.contains(&i)
                {
                    pages_to_render.push(i);
                }
            }
        }

        let layout_gen = self.layout_gen;
        for &page_idx in &pages_to_render {
            if let Some(page) = self.pages.get(page_idx)
                && page.svg_handle.is_none()
                && page.raster_handle.is_none()
                && let Some(display_list) = page.display_list.clone()
            {
                let index = page.index;
                if self.theme_colors {
                    let active = theme::active();
                    let cosmic = active.cosmic();
                    let text = cosmic.on_bg_color();
                    let bg = cosmic.bg_color();
                    let text_color = (
                        (text.color.red * 255.0) as u8,
                        (text.color.green * 255.0) as u8,
                        (text.color.blue * 255.0) as u8,
                    );
                    let bg_color = (
                        (bg.color.red * 255.0) as u8,
                        (bg.color.green * 255.0) as u8,
                        (bg.color.blue * 255.0) as u8,
                    );
                    tasks.push(Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                MuPdfViewerMessage::RasterReady(
                                    layout_gen,
                                    index,
                                    display_list_to_image_tinted(
                                        &display_list,
                                        2.0,
                                        text_color,
                                        bg_color,
                                    ),
                                )
                            })
                            .await
                            .unwrap()
                        },
                        cosmic::action::app,
                    ));
                } else {
                    tasks.push(Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                let svg = display_list.to_svg(&mupdf::Matrix::IDENTITY).unwrap();
                                MuPdfViewerMessage::SvgReady(
                                    layout_gen,
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

    /// Page indices currently visible on screen.  Returns one index in
    /// single-pane mode, or both pages of the current spread in dual-pane mode.
    fn visible_page_indices(&self) -> Vec<usize> {
        let (vw, _) = self.viewport_size.get();
        if self.should_dual_pane(vw) {
            let first = (self.active_page / 2) * 2;
            let mut pages = vec![first];
            if first + 1 < self.pages.len() {
                pages.push(first + 1);
            }
            pages
        } else {
            vec![self.active_page]
        }
    }
}

impl Page for MuPdfViewer {
    type Message = MuPdfViewerMessage;

    fn view(&self) -> Element<'_, MuPdfViewerMessage> {
        if self.file_path.is_none() {
            // No local source available
            let no_source = widget::Column::new()
                .align_x(cosmic::iced::Alignment::Center)
                .spacing(16)
                .push(
                    widget::icon::from_name("dialog-warning-symbolic")
                        .size(48)
                        .icon(),
                )
                .push(widget::text::body(fl!("pdf-viewer-no-local-source")));

            return widget::container(no_source)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(cosmic::iced::alignment::Horizontal::Center)
                .align_y(Vertical::Center)
                .into();
        }

        if let Some(error) = &self.load_error {
            let error_view = widget::Column::new()
                .align_x(cosmic::iced::Alignment::Center)
                .spacing(16)
                .push(
                    widget::icon::from_name("dialog-error-symbolic")
                        .size(48)
                        .icon(),
                )
                .push(widget::text::body(fl!(
                    "pdf-viewer-load-error",
                    error = error.as_str()
                )));

            return widget::container(error_view)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(cosmic::iced::alignment::Horizontal::Center)
                .align_y(Vertical::Center)
                .into();
        }

        if self.pages.is_empty() {
            // Loading state
            let loading = widget::Column::new()
                .align_x(cosmic::iced::Alignment::Center)
                .spacing(16)
                .push(
                    widget::icon::from_name("content-loading-symbolic")
                        .size(48)
                        .icon(),
                )
                .push(widget::text::body(fl!("pdf-viewer-loading")));

            return widget::container(loading)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(cosmic::iced::alignment::Horizontal::Center)
                .align_y(Vertical::Center)
                .into();
        }

        widget::responsive(move |size| {
            let show_pane = self.show_thumbnails && size.width >= MIN_WIDTH_WITH_THUMBNAILS;
            let content = self.view_content();

            if show_pane {
                widget::Row::new()
                    .push(self.view_thumbnails())
                    .push(content)
                    .height(Length::Fill)
                    .into()
            } else {
                let toggle_strip: Element<'_, MuPdfViewerMessage> = widget::Column::new()
                    .push(
                        widget::button::icon(
                            widget::icon::from_name("navbar-closed-symbolic").size(ICON_SIZE),
                        )
                        .on_press(MuPdfViewerMessage::ShowThumbnails(true)),
                    )
                    .push(widget::Space::new().height(Length::Fill))
                    .width(Length::Shrink)
                    .height(Length::Fill)
                    .into();

                widget::Row::new()
                    .push(toggle_strip)
                    .push(content)
                    .height(Length::Fill)
                    .into()
            }
        })
        .into()
    }

    fn view_header_center(&self) -> Vec<Element<'_, MuPdfViewerMessage>> {
        let first_path = self
            .document
            .contents
            .first()
            .and_then(|c| c.sources.first())
            .map(|s| s.path.as_str())
            .unwrap_or("PDF");
        let path = Path::new(first_path);
        let filename = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("PDF");

        vec![
            widget::text::heading(filename)
                .wrapping(cosmic::iced::widget::text::Wrapping::None)
                .into(),
        ]
    }

    fn view_header_start(&self) -> Vec<Element<'_, MuPdfViewerMessage>> {
        let cosmic_theme::Spacing { space_xxs, .. } = theme::active().cosmic().spacing;
        vec![
            widget::button::icon(widget::icon::from_name("go-previous-symbolic").size(ICON_SIZE))
                .on_press(MuPdfViewerMessage::Out(MuPdfViewerOutput::Close(
                    self.fingerprint.clone(),
                    if self.pages.is_empty() {
                        None
                    } else {
                        Some((self.active_page, self.pages.len()))
                    },
                )))
                .tooltip(fl!("pdf-viewer-back"))
                .padding(space_xxs)
                .into(),
        ]
    }

    fn view_header_end(&self) -> Vec<Element<'_, MuPdfViewerMessage>> {
        let cosmic_theme::Spacing { space_xxs, .. } = theme::active().cosmic().spacing;
        vec![
            widget::button::icon(
                widget::icon::from_name("document-properties-symbolic").size(ICON_SIZE),
            )
            .on_press(MuPdfViewerMessage::Out(
                MuPdfViewerOutput::OpenDocumentDetails(Box::new(self.document.clone())),
            ))
            .tooltip(fl!("pdf-viewer-document-details"))
            .padding(space_xxs)
            .into(),
        ]
    }

    fn view_context(&self) -> ContextView<'_, MuPdfViewerMessage> {
        let zoom_base = widget::settings::section()
            .title(fl!("pdf-viewer-zoom"))
            .add(
                widget::settings::item::builder(fl!("pdf-viewer-zoom")).control(widget::dropdown(
                    &self.zoom_names,
                    Zoom::all().iter().position(|z| z == &self.zoom),
                    MuPdfViewerMessage::ZoomDropdown,
                )),
            )
            .add(
                widget::settings::item::builder(fl!("pdf-viewer-theme-colors"))
                    .toggler(self.theme_colors, MuPdfViewerMessage::ThemeColors),
            )
            .add(
                widget::settings::item::builder(fl!("pdf-viewer-show-thumbnails"))
                    .toggler(self.show_thumbnails, MuPdfViewerMessage::ShowThumbnails),
            )
            .add(
                widget::settings::item::builder(fl!("pdf-viewer-dual-pane")).control(
                    widget::settings::item_row(vec![
                        widget::radio(
                            widget::text::body(fl!("pdf-viewer-dual-pane-off")),
                            DualPageMode::Off,
                            Some(self.dual_pane),
                            MuPdfViewerMessage::DualPane,
                        )
                        .into(),
                        widget::radio(
                            widget::text::body(fl!("pdf-viewer-dual-pane-auto")),
                            DualPageMode::Auto,
                            Some(self.dual_pane),
                            MuPdfViewerMessage::DualPane,
                        )
                        .into(),
                        widget::radio(
                            widget::text::body(fl!("pdf-viewer-dual-pane-on")),
                            DualPageMode::On,
                            Some(self.dual_pane),
                            MuPdfViewerMessage::DualPane,
                        )
                        .into(),
                    ])
                    .width(Length::Shrink),
                ),
            );

        let zoom_section = if self.is_reflowable {
            zoom_base.add(
                widget::settings::item::builder(fl!("pdf-viewer-epub-font-size")).control(
                    widget::slider(
                        8.0..=24.0,
                        self.epub_font_size,
                        MuPdfViewerMessage::EpubFontSize,
                    )
                    .step(1.0f32),
                ),
            )
        } else {
            zoom_base
        };

        let shortcuts_section = widget::settings::section()
            .title(fl!("pdf-viewer-keyboard-shortcuts"))
            .add(shortcut_item(
                "↑ ← PgUp",
                fl!("pdf-viewer-shortcut-previous-page"),
            ))
            .add(shortcut_item(
                "↓ → PgDn",
                fl!("pdf-viewer-shortcut-next-page"),
            ))
            .add(shortcut_item("0", fl!("pdf-viewer-shortcut-zoom-reset")))
            .add(shortcut_item("−", fl!("pdf-viewer-shortcut-zoom-out")))
            .add(shortcut_item("+", fl!("pdf-viewer-shortcut-zoom-in")))
            .add(shortcut_item("F", fl!("pdf-viewer-shortcut-fit-both")))
            .add(shortcut_item("H", fl!("pdf-viewer-shortcut-fit-height")))
            .add(shortcut_item("W", fl!("pdf-viewer-shortcut-fit-width")))
            .add(shortcut_item(
                "Ctrl+Scroll",
                fl!("pdf-viewer-shortcut-ctrl-scroll"),
            ));

        ContextView {
            title: fl!("pdf-viewer"),
            content: widget::settings::view_column(vec![
                zoom_section.into(),
                shortcuts_section.into(),
            ])
            .into(),
        }
    }

    fn update(&mut self, message: MuPdfViewerMessage) -> Task<Action<MuPdfViewerMessage>> {
        match message {
            MuPdfViewerMessage::ReadingProgressLoaded(page) => {
                self.initial_page = page;
                if !self.pages.is_empty()
                    && let Some(p) = page
                    && p < self.pages.len()
                {
                    self.active_page = p;
                    return self.update_active_page();
                }
                Task::none()
            }
            MuPdfViewerMessage::PageDiscovered(layout_gen, page) => {
                if layout_gen != self.layout_gen {
                    return Task::none();
                }
                self.pages.push(page);

                // If initial_page matches the just-discovered page, update active_page early
                if let Some(ip) = self.initial_page
                    && self.pages.len() - 1 == ip
                {
                    self.active_page = ip;
                }

                Task::none()
            }
            MuPdfViewerMessage::PagesDiscoveryComplete(layout_gen, is_reflowable) => {
                if layout_gen != self.layout_gen {
                    return Task::none();
                }
                self.is_reflowable = is_reflowable;
                if !self.pages.is_empty() {
                    // Final clamp in case initial_page arrived after discovery started
                    if let Some(ip) = self.initial_page {
                        self.active_page = ip.min(self.pages.len() - 1);
                    }
                    return self.update_active_page();
                }
                Task::none()
            }
            MuPdfViewerMessage::DisplayListReady(layout_gen, pdf_index, display_list) => {
                if layout_gen != self.layout_gen {
                    return Task::none();
                }
                if let Some(idx) = self.page_index_by_pdf_index(pdf_index) {
                    self.pages[idx].display_list = Some(display_list.clone());

                    let mut tasks = Vec::with_capacity(2);

                    // Generate thumbnail
                    let current_gen = self.layout_gen;
                    tasks.push(Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                let scale =
                                    (THUMBNAIL_WIDTH as f32) / display_list.bounds().width();
                                MuPdfViewerMessage::ThumbnailReady(
                                    current_gen,
                                    pdf_index,
                                    display_list_to_image(&display_list, scale),
                                )
                            })
                            .await
                            .unwrap()
                        },
                        cosmic::action::app,
                    ));

                    // Trigger rendering if this page is visible or within the
                    // raster prefetch window (next 2 pages).
                    let in_prefetch = self.theme_colors && idx.abs_diff(self.active_page) <= 2;
                    if self.visible_page_indices().contains(&idx) || in_prefetch {
                        tasks.push(self.update_active_page());
                    }

                    return Task::batch(tasks);
                }
                Task::none()
            }
            MuPdfViewerMessage::ThumbnailReady(layout_gen, pdf_index, handle) => {
                if layout_gen != self.layout_gen {
                    return Task::none();
                }
                if let Some(idx) = self.page_index_by_pdf_index(pdf_index) {
                    self.pages[idx].icon_handle = Some(handle);
                }
                Task::none()
            }
            MuPdfViewerMessage::SvgReady(layout_gen, pdf_index, handle) => {
                if layout_gen != self.layout_gen {
                    return Task::none();
                }
                if let Some(idx) = self.page_index_by_pdf_index(pdf_index) {
                    self.pages[idx].svg_handle = Some(handle);
                }
                Task::none()
            }
            MuPdfViewerMessage::RasterReady(layout_gen, pdf_index, handle) => {
                if layout_gen != self.layout_gen {
                    return Task::none();
                }
                if let Some(idx) = self.page_index_by_pdf_index(pdf_index) {
                    self.pages[idx].raster_handle = Some(handle);
                }
                Task::none()
            }
            MuPdfViewerMessage::SelectPage(idx) => {
                if idx < self.pages.len() {
                    self.active_page = idx;
                    return self.update_active_page();
                }
                Task::none()
            }
            MuPdfViewerMessage::PreviousPage => {
                let (vw, _) = self.viewport_size.get();
                let step = if self.should_dual_pane(vw) { 2 } else { 1 };
                self.active_page = self.active_page.saturating_sub(step);
                self.update_active_page()
            }
            MuPdfViewerMessage::NextPage => {
                let (vw, _) = self.viewport_size.get();
                let step = if self.should_dual_pane(vw) { 2 } else { 1 };
                if self.active_page + step < self.pages.len() {
                    self.active_page += step;
                }
                self.update_active_page()
            }
            MuPdfViewerMessage::ThumbnailScroll(viewport) => {
                self.thumbnail_viewport = Some(viewport);
                Task::none()
            }
            MuPdfViewerMessage::ZoomDropdown(index) => {
                if let Some(zoom) = Zoom::all().get(index) {
                    self.zoom = *zoom;
                }
                Task::none()
            }
            MuPdfViewerMessage::ZoomScroll(delta) => {
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
            MuPdfViewerMessage::Key(_modifiers, key, _text) => match &key {
                Key::Named(Named::ArrowUp | Named::ArrowLeft | Named::PageUp) => {
                    let (vw, _) = self.viewport_size.get();
                    let step = if self.should_dual_pane(vw) { 2 } else { 1 };
                    self.active_page = self.active_page.saturating_sub(step);
                    self.update_active_page()
                }
                Key::Named(Named::ArrowDown | Named::ArrowRight | Named::PageDown) => {
                    let (vw, _) = self.viewport_size.get();
                    let step = if self.should_dual_pane(vw) { 2 } else { 1 };
                    if self.active_page + step < self.pages.len() {
                        self.active_page += step;
                    }
                    self.update_active_page()
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
                    _ => Task::none(),
                },
                _ => Task::none(),
            },
            MuPdfViewerMessage::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers;
                Task::none()
            }
            MuPdfViewerMessage::ThemeColors(use_theme_colors) => {
                self.theme_colors = use_theme_colors;
                for page in &mut self.pages {
                    page.svg_handle = None;
                    page.raster_handle = None;
                }
                self.update_active_page()
            }
            MuPdfViewerMessage::ShowThumbnails(show_thumbnails) => {
                self.show_thumbnails = show_thumbnails;
                Task::none()
            }
            MuPdfViewerMessage::DualPane(dual_pane) => {
                self.dual_pane = dual_pane;
                self.update_active_page()
            }
            MuPdfViewerMessage::EpubFontSize(size) => {
                let size = size.clamp(8.0, 24.0);
                self.epub_font_size = size;
                save_mupdf_epub_font_size(size);
                self.repaginate()
            }
            MuPdfViewerMessage::LoadFailed(error) => {
                tracing::warn!("PDF load failed: {error}");
                self.load_error = Some(error);
                Task::none()
            }
            MuPdfViewerMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent component")
            }
        }
    }
}

fn shortcut_item<'a>(key: &'a str, description: String) -> Element<'a, MuPdfViewerMessage> {
    widget::settings::item::builder(description)
        .control(widget::text::monotext(key))
        .into()
}

/// Parse the page number from a progress JSON string like `{"page":5}`.
fn parse_page_from_progress(progress: &str) -> Option<usize> {
    // Simple parser to avoid serde_json dependency.
    let progress = progress.trim();
    let inner = progress.strip_prefix('{')?.strip_suffix('}')?;
    for part in inner.split(',') {
        let (key, value) = part.split_once(':')?;
        let key = key.trim().trim_matches('"');
        if key == "page" {
            return value.trim().parse::<usize>().ok();
        }
    }
    None
}
