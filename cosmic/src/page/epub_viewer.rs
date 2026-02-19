use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use cosmic::Action;
use cosmic::Element;
use cosmic::Task;
use cosmic::cosmic_theme;
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
use cosmic::iced::widget::span;
use cosmic::theme;
use cosmic::widget;
use epub::ContentBlock;
use epub::Document as EpubDocumentTrait;
use epub::EpubDocument;
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
    blocks: Vec<ContentBlock>,
}

// --- Messages ---

#[derive(Debug, Clone)]
pub enum EpubViewerOutput {
    Close(Fingerprint, Option<usize>),
}

#[derive(Clone, Debug)]
pub enum EpubViewerMessage {
    EpubLoaded(String, Vec<EpubChapter>),
    ReadingProgressLoaded(Option<usize>),
    SelectChapter(usize),
    ThemeColors(bool),
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
                    Ok(Some(progress)) => parse_chapter_from_progress(&progress.progress),
                    Ok(None) => None,
                    Err(e) => {
                        tracing::warn!("failed to load reading progress: {e}");
                        None
                    }
                }
            },
            |chapter| cosmic::action::app(EpubViewerMessage::ReadingProgressLoaded(chapter)),
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
                        Some(self.active_chapter)
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
                Task::none()
            }
            EpubViewerMessage::ReadingProgressLoaded(chapter) => {
                self.initial_chapter = chapter;
                if !self.chapters.is_empty()
                    && let Some(c) = chapter
                    && c < self.chapters.len()
                {
                    self.active_chapter = c;
                }
                Task::none()
            }
            EpubViewerMessage::SelectChapter(idx) => {
                if idx < self.chapters.len() {
                    self.active_chapter = idx;
                }
                Task::none()
            }
            EpubViewerMessage::ThemeColors(use_theme_colors) => {
                self.theme_colors = use_theme_colors;
                Task::none()
            }
            EpubViewerMessage::Key(_modifiers, key, _text) => match &key {
                Key::Named(Named::ArrowUp | Named::ArrowLeft | Named::PageUp) => {
                    if self.active_chapter > 0 {
                        self.active_chapter -= 1;
                    }
                    Task::none()
                }
                Key::Named(Named::ArrowDown | Named::ArrowRight | Named::PageDown) => {
                    if self.active_chapter + 1 < self.chapters.len() {
                        self.active_chapter += 1;
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

/// Parse the chapter number from a progress JSON string like `{"chapter":2}`.
fn parse_chapter_from_progress(progress: &str) -> Option<usize> {
    let progress = progress.trim();
    let inner = progress.strip_prefix('{')?.strip_suffix('}')?;
    for part in inner.split(',') {
        let (key, value) = part.split_once(':')?;
        let key = key.trim().trim_matches('"');
        if key == "chapter" {
            return value.trim().parse::<usize>().ok();
        }
    }
    None
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

    for (idx, item) in spine.iter().enumerate() {
        let label = if item.id.is_empty() {
            format!("Chapter {}", idx + 1)
        } else {
            item.id.clone()
        };

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

        chapters.push(EpubChapter { label, blocks });
    }

    (title, chapters)
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
        ContentBlock::Preformatted { text, .. } => {
            widget::text::monotext(text).width(Length::Fill).into()
        }
        ContentBlock::BlockQuote { children } => {
            let mut col = widget::column::with_capacity(children.len())
                .spacing(space_xxs)
                .width(Length::Fill);
            for child in children {
                col = col.push(render_block(child));
            }
            widget::container(col)
                .padding([0, 0, 0, space_s])
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
                .width(Length::Fill)
                .content_fit(cosmic::iced::ContentFit::ScaleDown)
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
        ContentBlock::HorizontalRule => widget::divider::horizontal::default().into(),
    }
}
