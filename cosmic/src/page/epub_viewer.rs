use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use cosmic::Action;
use cosmic::Element;
use cosmic::Task;
use cosmic::cosmic_theme;
use cosmic::iced::Length;
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::alignment::Vertical;
use cosmic::iced::core::SmolStr;
use cosmic::iced::keyboard::Key;
use cosmic::iced::keyboard::Modifiers;
use cosmic::iced::keyboard::key::Named;
use cosmic::theme;
use cosmic::widget;
use epub::ContentBlock;
use epub::Document as EpubDocumentTrait;
use epub::EpubDocument;

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

            widget::scrollable(widget::container(column).padding(space_s))
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
            content: widget::settings::view_column(vec![shortcuts_section.into()]).into(),
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
                epub::content::parse_xhtml(&data, href, &mut |img_path| {
                    match doc.resolve_resource(img_path) {
                        Ok(img_data) => {
                            let media_type = epub::content::guess_media_type(img_path);
                            Some((img_data, media_type))
                        }
                        Err(_) => None,
                    }
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

fn render_block(block: &ContentBlock) -> Element<'_, EpubViewerMessage> {
    let cosmic_theme::Spacing {
        space_xxs, space_s, ..
    } = theme::active().cosmic().spacing;

    match block {
        ContentBlock::Heading { level, text } => match level {
            1 => widget::text::title1(text).width(Length::Fill).into(),
            2 => widget::text::title2(text).width(Length::Fill).into(),
            3 => widget::text::title3(text).width(Length::Fill).into(),
            4 => widget::text::title4(text).width(Length::Fill).into(),
            _ => widget::text::heading(text).width(Length::Fill).into(),
        },
        ContentBlock::Paragraph { text } => {
            widget::text::body(text).width(Length::Fill).into()
        }
        ContentBlock::Preformatted { text } => {
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
                .padding([0, 0, 0, space_s as u16])
                .width(Length::Fill)
                .into()
        }
        ContentBlock::UnorderedList { items } => {
            let mut col = widget::column::with_capacity(items.len())
                .spacing(space_xxs)
                .width(Length::Fill);
            for item in items {
                col = col.push(
                    widget::text::body(format!("  \u{2022} {}", item.text)).width(Length::Fill),
                );
            }
            col.into()
        }
        ContentBlock::OrderedList { start, items } => {
            let mut col = widget::column::with_capacity(items.len())
                .spacing(space_xxs)
                .width(Length::Fill);
            for (i, item) in items.iter().enumerate() {
                let n = *start as usize + i;
                col = col.push(
                    widget::text::body(format!("  {n}. {}", item.text)).width(Length::Fill),
                );
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
                widget::text::body(format!("[{alt}]")).width(Length::Fill).into()
            } else {
                widget::Space::new(Length::Fill, 0).into()
            }
        }
        ContentBlock::HorizontalRule => widget::divider::horizontal::default().into(),
    }
}
