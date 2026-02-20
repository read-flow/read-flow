use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use archive_organizer::Builder;
use cosmic::Action;
use cosmic::Apply;
use cosmic::Element;
use cosmic::Task;
use cosmic::cosmic_theme;
use cosmic::iced::Background;
use cosmic::iced::Font;
use cosmic::iced::Length;
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::alignment::Vertical;
use cosmic::iced::core::SmolStr;
use cosmic::iced::font;
use cosmic::iced::keyboard::Key;
use cosmic::iced::keyboard::Modifiers;
use cosmic::iced::keyboard::key::Named;
use cosmic::iced::widget::rich_text;
use cosmic::iced::widget::scrollable;
use cosmic::iced::widget::span;
use cosmic::theme;
use cosmic::theme::Container;
use cosmic::widget;
use epub::ContentBlock;
use epub::Document as EpubDocumentTrait;
use epub::EpubDocument;
use epub::TableCell;
use epub::TextSpan;

use crate::ICON_SIZE;
use crate::aggregator::Document;
use crate::app::ContextView;
use crate::client::ClientSelector;
use crate::document_provider::DocumentProvider;
use crate::fl;
use crate::page::Page;

type Fingerprint = String;

const CHAPTER_SIDEBAR_WIDTH: f32 = 220.0;

// --- Core types ---

#[derive(Clone, Debug)]
pub(crate) struct EpubChapter {
    label: String,
    /// Resolved zip path for this spine item (e.g. `OEBPS/Text/ch1.xhtml`).
    href: String,
    blocks: Vec<ContentBlock>,
    /// Map of HTML anchor id → estimated absolute y-offset in pixels.
    /// Populated for `ContentBlock::Footnote` ids at chapter load time.
    anchors: HashMap<String, f32>,
}

// --- Messages ---

#[derive(Debug, Clone)]
pub enum EpubViewerOutput {
    /// Carries the fingerprint and the opaque progress JSON to persist, if any.
    Close(Fingerprint, Option<String>),
}

#[derive(Clone, Debug)]
pub enum EpubViewerMessage {
    EpubLoaded(String, Vec<EpubChapter>),
    /// Carries (chapter_index, scroll_y) restored from saved progress.
    ReadingProgressLoaded(Option<usize>, f32),
    SelectChapter(usize),
    ThemeColors(bool),
    Scrolled(scrollable::Viewport),
    FollowLink(String),
    Key(Modifiers, Key, Option<SmolStr>),
    ModifiersChanged(Modifiers),
    Out(EpubViewerOutput),
}

// --- EpubViewer page ---

pub struct EpubViewer {
    fingerprint: Fingerprint,
    document: Document,
    file_path: Option<PathBuf>,
    title: String,
    chapters: Vec<EpubChapter>,
    active_chapter: usize,
    initial_chapter: Option<usize>,
    /// Scroll position (absolute y offset in pixels) within the current chapter.
    scroll_y: f32,
    /// Scroll position saved before following a footnote fragment link.
    /// Used to navigate back when a back-reference link (e.g. `↩`) is clicked.
    scroll_before_jump: Option<f32>,
    modifiers: Modifiers,
    theme_colors: bool,
    content_scroll_id: widget::Id,
}

impl EpubViewer {
    pub fn new(
        document: Document,
        document_provider: Arc<DocumentProvider>,
    ) -> (Self, Task<Action<EpubViewerMessage>>) {
        let fingerprint = document.metadata.fingerprint.clone();

        let sources = document.sources_by_priority();
        let local_source = sources.iter().find(|s| s.client == ClientSelector::Local);
        let file_path = local_source.map(|s| PathBuf::from(&s.path));

        let viewer = EpubViewer {
            fingerprint: fingerprint.clone(),
            document,
            file_path: file_path.clone(),
            title: String::new(),
            chapters: Vec::new(),
            active_chapter: 0,
            initial_chapter: None,
            scroll_y: 0.0,
            scroll_before_jump: None,
            modifiers: Modifiers::default(),
            theme_colors: true,
            content_scroll_id: widget::Id::unique(),
        };

        let mut tasks = Vec::new();

        // Start loading the EPUB if we have a local path
        if let Some(path) = file_path {
            tasks.push(Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || load_epub_chapters(&path))
                        .await
                        .unwrap()
                },
                |(title, chapters)| {
                    cosmic::action::app(EpubViewerMessage::EpubLoaded(title, chapters))
                },
            ));
        }

        // Fetch reading progress
        let fp = fingerprint;
        tasks.push(Task::perform(
            async move {
                let aggregator = document_provider.aggregator.read().await;
                match aggregator.get_reading_progress(&fp).await {
                    Ok(Some(progress)) => parse_reading_progress(&progress.progress),
                    Ok(None) => (None, 0.0),
                    Err(e) => {
                        tracing::warn!("failed to load reading progress: {e}");
                        (None, 0.0)
                    }
                }
            },
            |(chapter, scroll_y)| {
                cosmic::action::app(EpubViewerMessage::ReadingProgressLoaded(chapter, scroll_y))
            },
        ));

        (viewer, Task::batch(tasks))
    }

    pub fn display_name(&self) -> String {
        if !self.title.is_empty() {
            return self.title.clone();
        }
        Path::new(&self.document.sources.iter().next().unwrap().path)
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("EPUB")
            .to_string()
    }

    fn view_chapter_sidebar(&self) -> Element<'_, EpubViewerMessage> {
        let cosmic_theme::Spacing { space_xxs, .. } = theme::active().cosmic().spacing;

        let chapter_info = if !self.chapters.is_empty() {
            format!("{} / {}", self.active_chapter + 1, self.chapters.len())
        } else {
            String::new()
        };

        let mut column = widget::column::with_capacity(self.chapters.len())
            .padding(space_xxs)
            .spacing(space_xxs);

        for (idx, chapter) in self.chapters.iter().enumerate() {
            let label = widget::text::body(&chapter.label)
                .wrapping(cosmic::iced::widget::text::Wrapping::None);
            let button = widget::button::custom(label)
                .on_press(EpubViewerMessage::SelectChapter(idx))
                .selected(idx == self.active_chapter)
                .width(Length::Fill);
            column = column.push(button);
        }

        widget::Column::with_children(vec![
            widget::Column::with_children(vec![
                widget::text::body(chapter_info)
                    .wrapping(cosmic::iced::widget::text::Wrapping::None)
                    .into(),
            ])
            .width(Length::Fill)
            .align_x(Horizontal::Center)
            .into(),
            widget::scrollable(column).width(Length::Fill).into(),
        ])
        .width(Length::Fixed(CHAPTER_SIDEBAR_WIDTH))
        .height(Length::Fill)
        .into()
    }

    fn view_content(&self) -> Element<'_, EpubViewerMessage> {
        let cosmic_theme::Spacing {
            space_s, space_xxs, ..
        } = theme::active().cosmic().spacing;

        if let Some(chapter) = self.chapters.get(self.active_chapter) {
            let mut column = widget::column::with_capacity(chapter.blocks.len())
                .spacing(space_xxs)
                .width(Length::Fill);

            for block in &chapter.blocks {
                column = column.push(render_block(block));
            }

            let theme_colors = self.theme_colors;

            // Inner "paper" container with max-width for readability
            let paper =
                widget::container(widget::container(column).padding(space_s).max_width(800.0))
                    .style(move |theme: &cosmic::Theme| {
                        if theme_colors {
                            let c = theme.cosmic().bg_color();
                            widget::container::background(cosmic::iced::Color::from_rgba(
                                c.color.red,
                                c.color.green,
                                c.color.blue,
                                c.alpha,
                            ))
                        } else {
                            widget::container::background(cosmic::iced::Color::WHITE)
                        }
                    })
                    .width(Length::Fill)
                    .align_x(Horizontal::Center);

            // Outer "desk" container
            let outer = widget::container(paper)
                .style(|theme: &cosmic::Theme| {
                    let c = theme.cosmic().bg_component_color();
                    widget::container::background(cosmic::iced::Color::from_rgba(
                        c.color.red,
                        c.color.green,
                        c.color.blue,
                        c.alpha,
                    ))
                })
                .width(Length::Fill);

            widget::scrollable(outer)
                .id(self.content_scroll_id.clone())
                .on_scroll(EpubViewerMessage::Scrolled)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else {
            widget::Space::new(Length::Fill, Length::Fill).into()
        }
    }
}

impl Page for EpubViewer {
    type Message = EpubViewerMessage;

    fn view(&self) -> Element<'_, EpubViewerMessage> {
        if self.file_path.is_none() {
            let no_source = widget::column()
                .align_x(cosmic::iced::Alignment::Center)
                .spacing(16)
                .push(
                    widget::icon::from_name("dialog-warning-symbolic")
                        .size(48)
                        .icon(),
                )
                .push(widget::text::body(fl!("epub-viewer-no-local-source")));

            return widget::container(no_source)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Center)
                .into();
        }

        if self.chapters.is_empty() {
            let loading = widget::column()
                .align_x(cosmic::iced::Alignment::Center)
                .spacing(16)
                .push(
                    widget::icon::from_name("content-loading-symbolic")
                        .size(48)
                        .icon(),
                )
                .push(widget::text::body(fl!("epub-viewer-loading")));

            return widget::container(loading)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Center)
                .into();
        }

        let sidebar = self.view_chapter_sidebar();
        let content = self.view_content();

        widget::row()
            .push(sidebar)
            .push(content)
            .height(Length::Fill)
            .into()
    }

    fn view_header_center(&self) -> Vec<Element<'_, EpubViewerMessage>> {
        vec![
            widget::text::heading(self.display_name())
                .wrapping(cosmic::iced::widget::text::Wrapping::None)
                .into(),
        ]
    }

    fn view_header_end(&self) -> Vec<Element<'_, EpubViewerMessage>> {
        let cosmic_theme::Spacing { space_xxs, .. } = theme::active().cosmic().spacing;

        vec![
            widget::button::icon(widget::icon::from_name("window-close-symbolic").size(ICON_SIZE))
                .on_press(EpubViewerMessage::Out(EpubViewerOutput::Close(
                    self.fingerprint.clone(),
                    if self.chapters.is_empty() {
                        None
                    } else {
                        Some(serialize_progress(self.active_chapter, self.scroll_y))
                    },
                )))
                .tooltip(fl!("epub-viewer-back"))
                .padding(space_xxs)
                .into(),
        ]
    }

    fn view_context(&self) -> ContextView<'_, EpubViewerMessage> {
        let display_section = widget::settings::section()
            .title(fl!("epub-viewer-display"))
            .add(
                widget::settings::item::builder(fl!("epub-viewer-theme-colors"))
                    .toggler(self.theme_colors, EpubViewerMessage::ThemeColors),
            );

        let shortcuts_section = widget::settings::section()
            .title(fl!("epub-viewer-keyboard-shortcuts"))
            .add(shortcut_item(
                "↑ ← PgUp",
                fl!("epub-viewer-shortcut-previous-chapter"),
            ))
            .add(shortcut_item(
                "↓ → PgDn",
                fl!("epub-viewer-shortcut-next-chapter"),
            ));

        ContextView {
            title: self.display_name(),
            content: widget::settings::view_column(vec![
                display_section.into(),
                shortcuts_section.into(),
            ])
            .into(),
        }
    }

    fn update(&mut self, message: EpubViewerMessage) -> Task<Action<EpubViewerMessage>> {
        match message {
            EpubViewerMessage::EpubLoaded(title, chapters) => {
                self.title = title;
                self.chapters = chapters;
                if !self.chapters.is_empty() {
                    self.active_chapter = self
                        .initial_chapter
                        .unwrap_or(0)
                        .min(self.chapters.len() - 1);
                }
                // Restore scroll position once content is available
                if self.scroll_y > 0.0 {
                    scrollable::scroll_to(
                        self.content_scroll_id.clone(),
                        scrollable::AbsoluteOffset {
                            x: 0.0,
                            y: self.scroll_y,
                        },
                    )
                } else {
                    Task::none()
                }
            }
            EpubViewerMessage::ReadingProgressLoaded(chapter, scroll_y) => {
                self.initial_chapter = chapter;
                self.scroll_y = scroll_y;
                if !self.chapters.is_empty()
                    && let Some(c) = chapter
                    && c < self.chapters.len()
                {
                    self.active_chapter = c;
                }
                // Restore scroll if chapters are already loaded
                if self.scroll_y > 0.0 && !self.chapters.is_empty() {
                    scrollable::scroll_to(
                        self.content_scroll_id.clone(),
                        scrollable::AbsoluteOffset {
                            x: 0.0,
                            y: self.scroll_y,
                        },
                    )
                } else {
                    Task::none()
                }
            }
            EpubViewerMessage::SelectChapter(idx) => {
                if idx < self.chapters.len() {
                    self.active_chapter = idx;
                    self.scroll_y = 0.0;
                    self.scroll_before_jump = None;
                }
                Task::none()
            }
            EpubViewerMessage::ThemeColors(use_theme_colors) => {
                self.theme_colors = use_theme_colors;
                Task::none()
            }
            EpubViewerMessage::Scrolled(viewport) => {
                self.scroll_y = viewport.absolute_offset().y;
                Task::none()
            }
            EpubViewerMessage::FollowLink(href) => {
                let (path, fragment) = match href.split_once('#') {
                    Some((p, f)) => (p, Some(f)),
                    None => (href.as_str(), None),
                };

                if path.is_empty() {
                    // Pure fragment link — same-page navigation.
                    if let Some(frag) = fragment.filter(|f| !f.is_empty()) {
                        if let Some(chapter) = self.chapters.get(self.active_chapter) {
                            if let Some(&target_y) = chapter.anchors.get(frag) {
                                // Navigating to a footnote: save reading position (once).
                                if self.scroll_before_jump.is_none() {
                                    self.scroll_before_jump = Some(self.scroll_y);
                                }
                                return scrollable::scroll_to(
                                    self.content_scroll_id.clone(),
                                    scrollable::AbsoluteOffset {
                                        x: 0.0,
                                        y: target_y,
                                    },
                                );
                            } else if let Some(saved_y) = self.scroll_before_jump.take() {
                                // Unknown fragment (likely a back-reference ↩): restore
                                // the position saved before the last footnote jump.
                                return scrollable::scroll_to(
                                    self.content_scroll_id.clone(),
                                    scrollable::AbsoluteOffset { x: 0.0, y: saved_y },
                                );
                            }
                        }
                    }
                    return Task::none();
                }

                // Cross-chapter link: resolve and navigate.
                if let Some(current) = self.chapters.get(self.active_chapter) {
                    let base = epub::content::base_dir(&current.href);
                    let resolved = epub::content::resolve_href(base, path);
                    if let Some(idx) = self
                        .chapters
                        .iter()
                        .position(|c| c.href == resolved || c.href.ends_with(&resolved))
                    {
                        self.active_chapter = idx;
                        self.scroll_y = 0.0;
                        self.scroll_before_jump = None;
                    }
                }
                Task::none()
            }
            EpubViewerMessage::Key(_modifiers, key, _text) => match &key {
                Key::Named(Named::ArrowUp | Named::ArrowLeft | Named::PageUp) => {
                    if self.active_chapter > 0 {
                        self.active_chapter -= 1;
                        self.scroll_y = 0.0;
                    }
                    Task::none()
                }
                Key::Named(Named::ArrowDown | Named::ArrowRight | Named::PageDown) => {
                    if self.active_chapter + 1 < self.chapters.len() {
                        self.active_chapter += 1;
                        self.scroll_y = 0.0;
                    }
                    Task::none()
                }
                _ => Task::none(),
            },
            EpubViewerMessage::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers;
                Task::none()
            }
            EpubViewerMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent component")
            }
        }
    }
}

fn shortcut_item<'a>(key: &'a str, description: String) -> Element<'a, EpubViewerMessage> {
    widget::settings::item::builder(description)
        .control(widget::text::monotext(key))
        .into()
}

/// Serialize reading progress to a JSON string.
fn serialize_progress(chapter: usize, scroll_y: f32) -> String {
    format!("{{\"chapter\":{chapter},\"scroll\":{scroll_y}}}")
}

/// Parse reading progress from a JSON string like `{"chapter":2,"scroll":340.5}`.
/// Returns `(chapter_index, scroll_y)`.
fn parse_reading_progress(progress: &str) -> (Option<usize>, f32) {
    let mut chapter = None;
    let mut scroll_y = 0.0f32;
    let progress = progress.trim();
    if let Some(inner) = progress.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        for part in inner.split(',') {
            if let Some((key, value)) = part.split_once(':') {
                match key.trim().trim_matches('"') {
                    "chapter" => chapter = value.trim().parse().ok(),
                    "scroll" => scroll_y = value.trim().parse().unwrap_or(0.0),
                    _ => {}
                }
            }
        }
    }
    (chapter, scroll_y)
}

/// Load EPUB chapters from a file path. Runs on a blocking thread.
fn load_epub_chapters(path: &Path) -> (String, Vec<EpubChapter>) {
    let doc = match EpubDocument::open(path) {
        Ok(doc) => doc,
        Err(e) => {
            tracing::error!("failed to open EPUB: {e}");
            return (String::new(), Vec::new());
        }
    };

    let title = doc.metadata().title.clone().unwrap_or_default();

    let spine = doc.spine().to_vec();
    let mut chapters = Vec::with_capacity(spine.len());

    for (idx, item) in spine.iter().filter(|item| item.linear).enumerate() {
        let label = item
            .label
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| item.id.clone())
            .apply_when(String::is_empty, |_| format!("Chapter {}", idx + 1));

        let blocks = match doc.resolve_resource(&item.href) {
            Ok(data) => {
                let href = &item.href;
                epub::content::parse_xhtml(&data, href, &mut |img_path| match doc
                    .resolve_resource(img_path)
                {
                    Ok(img_data) => {
                        let media_type = epub::content::guess_media_type(img_path);
                        Some((img_data, media_type))
                    }
                    Err(_) => None,
                })
            }
            Err(e) => {
                tracing::warn!("failed to resolve spine item {}: {e}", item.href);
                Vec::new()
            }
        };

        let anchors = build_anchor_map(&blocks);
        chapters.push(EpubChapter {
            label,
            href: item.href.clone(),
            blocks,
            anchors,
        });
    }

    (title, chapters)
}

/// Estimate the rendered height of a block in pixels.
/// Used to build approximate scroll-anchor positions for footnote navigation.
fn estimated_block_height(block: &ContentBlock) -> f32 {
    match block {
        ContentBlock::Heading { level: 1, .. } => 56.0,
        ContentBlock::Heading { level: 2, .. } => 48.0,
        ContentBlock::Heading { level: 3, .. } => 40.0,
        ContentBlock::Heading { .. } => 32.0,
        ContentBlock::Paragraph { text, .. } => {
            // ~80 chars per line at default width, ~22px per line
            ((text.len() as f32 / 80.0).ceil() * 22.0).max(22.0)
        }
        ContentBlock::Preformatted { text, .. } => (text.lines().count() as f32 * 20.0).max(20.0),
        ContentBlock::BlockQuote { children } => {
            children.iter().map(estimated_block_height).sum::<f32>() + 16.0
        }
        ContentBlock::UnorderedList { items } => items.len() as f32 * 28.0,
        ContentBlock::OrderedList { items, .. } => items.len() as f32 * 28.0,
        ContentBlock::Image { .. } => 200.0,
        ContentBlock::Table { rows } => rows.len() as f32 * 36.0 + 8.0,
        ContentBlock::HorizontalRule => 16.0,
        ContentBlock::Footnote { blocks, .. } => {
            blocks.iter().map(estimated_block_height).sum::<f32>() + 16.0
        }
    }
}

/// Build a map of HTML anchor id → estimated absolute y-offset (pixels).
/// Covers `ContentBlock::Footnote` ids.
fn build_anchor_map(blocks: &[ContentBlock]) -> HashMap<String, f32> {
    const SPACING: f32 = 8.0;
    let mut map = HashMap::new();
    let mut y = 0.0f32;
    for block in blocks {
        if let ContentBlock::Footnote { id, .. } = block {
            if !id.is_empty() {
                map.insert(id.clone(), y);
            }
        }
        y += estimated_block_height(block) + SPACING;
    }
    map
}

fn styled_span<'a>(
    text_span: &'a TextSpan,
) -> cosmic::iced::widget::text::Span<'a, EpubViewerMessage> {
    let style = &text_span.style;
    let mut s = span(text_span.text.as_str());

    let weight = if style.bold {
        font::Weight::Bold
    } else {
        font::Weight::Normal
    };
    let font_style = if style.italic {
        font::Style::Italic
    } else {
        font::Style::Normal
    };
    s = s.font(Font {
        weight,
        style: font_style,
        ..Font::default()
    });

    if style.underline {
        s = s.underline(true);
    }
    if style.strikethrough {
        s = s.strikethrough(true);
    }
    if style.monospaced {
        s = s.font(cosmic::font::mono());
        s = s.background(Background::Color(
            cosmic::theme::active().cosmic().secondary.base.into(),
        ));
    }
    if let Some(href) = &text_span.link {
        s = s.link(EpubViewerMessage::FollowLink(href.clone()));
        s = s.color(theme::active().cosmic().accent_color());
    }
    s
}

fn render_spans(spans: &[TextSpan], size: f32) -> Element<'_, EpubViewerMessage> {
    let iced_spans: Vec<_> = spans.iter().map(styled_span).collect();
    rich_text(iced_spans).size(size).width(Length::Fill).into()
}

fn render_list_item_spans<'a>(
    prefix: String,
    spans: &'a [TextSpan],
    size: f32,
) -> Element<'a, EpubViewerMessage> {
    let mut iced_spans: Vec<cosmic::iced::widget::text::Span<'a, EpubViewerMessage>> =
        Vec::with_capacity(spans.len() + 1);
    iced_spans.push(span(prefix));
    iced_spans.extend(spans.iter().map(styled_span));
    rich_text(iced_spans).size(size).width(Length::Fill).into()
}

fn render_block(block: &ContentBlock) -> Element<'_, EpubViewerMessage> {
    let cosmic_theme::Spacing {
        space_xxs, space_s, ..
    } = theme::active().cosmic().spacing;

    match block {
        ContentBlock::Heading { level, spans, text } => {
            if spans.is_empty() {
                return match level {
                    1 => widget::text::title1(text).width(Length::Fill).into(),
                    2 => widget::text::title2(text).width(Length::Fill).into(),
                    3 => widget::text::title3(text).width(Length::Fill).into(),
                    4 => widget::text::title4(text).width(Length::Fill).into(),
                    _ => widget::text::heading(text).width(Length::Fill).into(),
                };
            }
            let size = match level {
                1 => 32.0,
                2 => 28.0,
                3 => 24.0,
                4 => 20.0,
                _ => 18.0,
            };
            render_spans(spans, size)
        }
        ContentBlock::Paragraph { spans, text } => {
            if spans.is_empty() {
                return widget::text::body(text).width(Length::Fill).into();
            }
            render_spans(spans, 16.0)
        }
        ContentBlock::Preformatted { text, .. } => widget::text::monotext(text)
            .width(Length::Fill)
            .apply(widget::container)
            .padding([space_xxs, space_s])
            .class(Container::Secondary)
            .into(),
        ContentBlock::BlockQuote { children } => {
            let mut col = widget::column::with_capacity(children.len())
                .spacing(space_xxs)
                .width(Length::Fill);
            for child in children {
                col = col.push(render_block(child));
            }
            widget::container(col)
                .padding([space_xxs, space_s])
                .width(Length::Fill)
                .into()
        }
        ContentBlock::UnorderedList { items } => {
            let mut col = widget::column::with_capacity(items.len())
                .spacing(space_xxs)
                .width(Length::Fill);
            for item in items {
                if item.spans.is_empty() {
                    col = col.push(
                        widget::text::body(format!("  \u{2022} {}", item.text)).width(Length::Fill),
                    );
                } else {
                    col = col.push(render_list_item_spans(
                        "  \u{2022} ".to_string(),
                        &item.spans,
                        16.0,
                    ));
                }
            }
            col.into()
        }
        ContentBlock::OrderedList { start, items } => {
            let mut col = widget::column::with_capacity(items.len())
                .spacing(space_xxs)
                .width(Length::Fill);
            for (i, item) in items.iter().enumerate() {
                let n = *start as usize + i;
                if item.spans.is_empty() {
                    col = col.push(
                        widget::text::body(format!("  {n}. {}", item.text)).width(Length::Fill),
                    );
                } else {
                    let prefix = format!("  {n}. ");
                    col = col.push(render_list_item_spans(prefix, &item.spans, 16.0));
                }
            }
            col.into()
        }
        ContentBlock::Image { data, .. } if !data.is_empty() => {
            let handle = widget::image::Handle::from_bytes(data.clone());
            widget::image(handle)
                .width(Length::Shrink)
                .content_fit(cosmic::iced::ContentFit::ScaleDown)
                .apply(widget::container)
                .width(Length::Fill)
                .align_x(Horizontal::Center)
                .into()
        }
        ContentBlock::Image { alt, .. } => {
            if !alt.is_empty() {
                widget::text::body(format!("[{alt}]"))
                    .width(Length::Fill)
                    .into()
            } else {
                widget::Space::new(Length::Fill, 0).into()
            }
        }
        ContentBlock::Table { rows } => render_table(rows),
        ContentBlock::HorizontalRule => widget::divider::horizontal::default().into(),
        ContentBlock::Footnote { blocks, .. } => render_footnote(blocks),
    }
}

fn render_footnote(blocks: &[ContentBlock]) -> Element<'_, EpubViewerMessage> {
    let cosmic_theme::Spacing {
        space_xxs, space_s, ..
    } = theme::active().cosmic().spacing;

    let mut col = widget::column::with_capacity(blocks.len())
        .spacing(space_xxs)
        .width(Length::Fill);

    for block in blocks {
        let el: Element<_> = match block {
            ContentBlock::Paragraph { spans, text } => {
                if spans.is_empty() {
                    widget::text::caption(text).width(Length::Fill).into()
                } else {
                    render_spans(spans, 13.0)
                }
            }
            _ => render_block(block),
        };
        col = col.push(el);
    }

    widget::container(col)
        .padding([space_xxs, space_s])
        .class(Container::Secondary)
        .width(Length::Fill)
        .into()
}

fn render_table(rows: &[Vec<TableCell>]) -> Element<'_, EpubViewerMessage> {
    let cosmic_theme::Spacing {
        space_xxs, space_s, ..
    } = theme::active().cosmic().spacing;

    let mut col = widget::column::with_capacity(rows.len() + 1)
        .spacing(0)
        .width(Length::Fill);

    let mut seen_header = false;
    let mut divider_inserted = false;

    for row in rows {
        let is_header_row = row.iter().any(|c| c.is_header);

        // Insert divider between header section and body section
        if seen_header && !is_header_row && !divider_inserted {
            col = col.push(widget::divider::horizontal::default());
            divider_inserted = true;
        }
        if is_header_row {
            seen_header = true;
        }

        let mut row_widget = widget::row().width(Length::Fill);

        for cell in row {
            let cell_spans: Vec<cosmic::iced::widget::text::Span<'_, EpubViewerMessage>> =
                if !cell.spans.is_empty() {
                    cell.spans.iter().map(styled_span).collect()
                } else if !cell.text.is_empty() {
                    let mut s = span(cell.text.as_str());
                    if cell.is_header {
                        s = s.font(Font {
                            weight: font::Weight::Bold,
                            ..Font::default()
                        });
                    }
                    vec![s]
                } else {
                    vec![]
                };

            let cell_content: Element<'_, EpubViewerMessage> = if !cell_spans.is_empty() {
                rich_text(cell_spans).size(16.0).width(Length::Fill).into()
            } else {
                widget::Space::new(Length::Fill, 0).into()
            };

            row_widget = row_widget.push(
                widget::container(cell_content)
                    .padding([space_xxs, space_s])
                    .width(Length::FillPortion(1)),
            );
        }

        col = col.push(row_widget);
    }

    widget::container(col).width(Length::Fill).into()
}
