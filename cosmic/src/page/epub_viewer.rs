use std::cell::Cell;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use archive_organizer::Builder;
use cosmic::Action;
use cosmic::Apply;
use cosmic::Element;
use cosmic::Task;
use cosmic::cosmic_config;
use cosmic::cosmic_config::ConfigGet;
use cosmic::cosmic_config::ConfigSet;
use cosmic::cosmic_theme;
use cosmic::iced::Background;
use cosmic::iced::Border;
use cosmic::iced::Color;
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
use cosmic_text::FontSystem;
use epub::BlockStyle;
use epub::ContentBlock;
use epub::Document as EpubDocumentTrait;
use epub::EpubDocument;
use epub::NavEntry;
use epub::StyleSheet;
use epub::TableCell;
use epub::TextAlign;
use epub::TextSpan;

use crate::ICON_SIZE;
use crate::aggregator::Document;
use crate::app::ContextView;
use crate::client::ClientSelector;
use crate::document_provider::DocumentProvider;
use crate::fl;
use crate::fonts::fonts;
use crate::layout::full_page;
use crate::page::Page;

// --- Block highlight ---

/// How a block should be visually highlighted.
#[derive(Clone, Copy, PartialEq, Eq)]
enum BlockHighlight {
    /// No highlight.
    None,
    /// Subtle tint — a search match that is not the current one.
    SearchMatch,
    /// Strong accent — the current search match or a nav-target flash.
    Current,
}

type Fingerprint = String;

const CHAPTER_SIDEBAR_WIDTH: f32 = 220.0;

// --- Persistent reader preferences ---

/// APP_ID used when accessing the cosmic-config store.
const APP_ID: &str = "com.github.peterpaul.archive-organizer-cosmic";

/// Config version for EPUB reader preferences (individual key access).
const EPUB_PREFS_VERSION: u64 = 1;

/// Config key for the saved font family.
const KEY_FONT_FAMILY: &str = "epub_font_family";

/// Config key for the saved base font size (stored as integer pixels).
const KEY_BASE_FONT_SIZE: &str = "epub_base_font_size";

/// Convert a `FontFamily` to the string stored in config.
fn font_family_to_str(family: FontFamily) -> String {
    match family {
        FontFamily::SansSerif => "SansSerif".to_string(),
        FontFamily::Serif => "Serif".to_string(),
        FontFamily::Monospace => "Monospace".to_string(),
        FontFamily::Named(name) => name.to_string(),
    }
}

/// Convert a config string back to `FontFamily`.
/// Named fonts are matched against the available system fonts; unrecognised
/// strings fall back to the default.
fn str_to_font_family(s: &str) -> FontFamily {
    match s {
        "SansSerif" => FontFamily::SansSerif,
        "Serif" | "" => FontFamily::Serif,
        "Monospace" => FontFamily::Monospace,
        name => fonts()
            .into_iter()
            .find(|&f| f == name)
            .map(FontFamily::Named)
            .unwrap_or_default(),
    }
}

/// Load saved EPUB reader font preferences from cosmic-config.
/// Returns `(font_family, base_font_size_px)`.
fn load_epub_font_prefs() -> (FontFamily, f32) {
    let Ok(ctx) = cosmic_config::Config::new(APP_ID, EPUB_PREFS_VERSION) else {
        return (FontFamily::default(), 16.0);
    };
    let family_str: String = ctx.get(KEY_FONT_FAMILY).unwrap_or_default();
    let size_px: u32 = ctx.get(KEY_BASE_FONT_SIZE).unwrap_or(16);
    let font_size = (size_px as f32).clamp(12.0, 24.0);
    (str_to_font_family(&family_str), font_size)
}

/// Save EPUB reader font preferences to cosmic-config.
fn save_epub_font_prefs(font_family: FontFamily, base_font_size: f32) {
    let Ok(ctx) = cosmic_config::Config::new(APP_ID, EPUB_PREFS_VERSION) else {
        return;
    };
    let _ = ctx.set(KEY_FONT_FAMILY, font_family_to_str(font_family));
    let _ = ctx.set(KEY_BASE_FONT_SIZE, base_font_size.round() as u32);
}

// --- View mode and pagination types ---

/// Which reading mode the viewer uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ViewMode {
    Scroll,
    #[default]
    Paginated,
}

/// A page is a contiguous range of blocks from the chapter's block list.
/// Stored as \[start .. end) (exclusive end).
#[derive(Clone, Debug)]
struct PageRange {
    start: usize,
    end: usize,
}

/// Cached pagination layout for a chapter at a particular viewport size.
#[derive(Clone, Debug)]
struct PaginationLayout {
    /// The available content height (in pixels) used to compute pages.
    page_height: f32,
    /// The available content width (in pixels) used for height estimation.
    page_width: f32,
    /// Computed page ranges.
    pages: Vec<PageRange>,
}

/// Whether to display two pages side by side in paginated mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DualPageMode {
    #[default]
    Auto,
    Off,
    On,
}

/// Font family for rendering EPUB content.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum FontFamily {
    SansSerif,
    #[default]
    Serif,
    Monospace,
    Named(&'static str),
}

impl fmt::Display for FontFamily {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label())
    }
}

impl FontFamily {
    fn all() -> Vec<FontFamily> {
        [
            FontFamily::SansSerif,
            FontFamily::Serif,
            FontFamily::Monospace,
        ]
        .into_iter()
        .chain(fonts().into_iter().map(FontFamily::Named))
        .collect()
    }

    fn to_family(self) -> font::Family {
        match self {
            FontFamily::SansSerif => font::Family::SansSerif,
            FontFamily::Serif => font::Family::Serif,
            FontFamily::Monospace => font::Family::Monospace,
            FontFamily::Named(name) => font::Family::Name(name),
        }
    }

    fn label(self) -> &'static str {
        match self {
            FontFamily::SansSerif => "Sans Serif",
            FontFamily::Serif => "Serif",
            FontFamily::Monospace => "Monospace",
            FontFamily::Named(name) => name,
        }
    }
}

/// Position saved before following a footnote link, for back-navigation.
#[derive(Clone, Debug)]
enum SavedPosition {
    ScrollY(f32),
    PageIndex(usize),
}

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
    /// Raw XHTML source for debug display.
    raw_html: String,
}

// Cloneable wrapper for EpubDocument
#[derive(Clone)]
pub(crate) struct CloneableEpubDocument(Arc<EpubDocument>);

unsafe impl Send for CloneableEpubDocument {}
unsafe impl Sync for CloneableEpubDocument {}

impl std::fmt::Debug for CloneableEpubDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CloneableEpubDocument(...)")
    }
}

impl CloneableEpubDocument {
    fn as_ref(&self) -> &EpubDocument {
        &self.0
    }
}

impl From<EpubDocument> for CloneableEpubDocument {
    fn from(doc: EpubDocument) -> Self {
        Self(Arc::new(doc))
    }
}

// --- Messages ---

#[derive(Debug)]
pub enum EpubViewerOutput {
    /// Carries the fingerprint and the opaque progress JSON to persist, if any.
    Close(Fingerprint, Option<String>),
}

impl Clone for EpubViewerOutput {
    fn clone(&self) -> Self {
        match self {
            EpubViewerOutput::Close(fp, progress) => {
                EpubViewerOutput::Close(fp.clone(), progress.clone())
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum EpubViewerMessage {
    EpubLoaded(String, Vec<EpubChapter>, CloneableEpubDocument),
    /// Carries restored reading position from saved progress.
    ReadingProgressLoaded(ReadingPosition),
    SelectChapter(usize),
    SelectNavEntry(usize),
    ShowRawHtml(bool),
    Scrolled(scrollable::Viewport),
    FollowLink(String),
    Key(Modifiers, Key, Option<SmolStr>),
    ModifiersChanged(Modifiers),
    /// Switch between scroll and paginated view mode.
    SetViewMode(ViewMode),
    /// Navigate to the next page (may cross chapter boundary).
    NextPage,
    /// Navigate to the previous page (may cross chapter boundary).
    PreviousPage,
    /// Toggle dual-page display mode.
    SetDualPage(DualPageMode),
    /// Set the page height fraction (0.5..=1.0) for pagination.
    SetPageHeightFraction(f32),
    /// Toggle chapter navigation sidebar visibility.
    ShowSidebar(bool),
    /// Set content column max-width as a percentage of the default (50..=150).
    SetContentMaxWidth(f32),
    /// Set the font family for rendering (index into FontFamily::ALL).
    SetFontFamily(FontFamily),
    /// Set the base body font size in pixels (12–24).
    SetBaseFontSize(f32),
    Out(EpubViewerOutput),
    /// Clear the navigation-target block highlight after the flash timer expires.
    ClearHighlight,
    /// Toggle the search bar open/closed.
    ToggleSearch,
    /// Update the search query and recompute matches.
    SetSearchQuery(String),
    /// Navigate to the next search match.
    SearchNext,
    /// Navigate to the previous search match.
    SearchPrevious,
    /// Close and clear the search bar.
    CloseSearch,
}

// --- EpubViewer page ---

pub struct EpubViewer {
    fingerprint: Fingerprint,
    document: Document,
    epub_document: Option<CloneableEpubDocument>,
    file_path: Option<PathBuf>,
    title: String,
    chapters: Vec<EpubChapter>,
    active_chapter: usize,
    initial_chapter: Option<usize>,
    /// Scroll position (absolute y offset in pixels) within the current chapter.
    scroll_y: f32,
    /// Position saved before following a footnote fragment link.
    /// Used to navigate back when a back-reference link (e.g. `↩`) is clicked.
    saved_position: Option<SavedPosition>,
    modifiers: Modifiers,
    show_raw_html: bool,
    content_scroll_id: widget::Id,
    /// Current reading mode: scroll (default) or paginated.
    view_mode: ViewMode,
    /// In paginated mode, the current page index within the active chapter.
    current_page: usize,
    /// Cached pagination layout per chapter index.
    pagination_cache: HashMap<usize, PaginationLayout>,
    /// Most recently observed viewport dimensions, set from the `responsive`
    /// closure (via Cell, since `view()` takes `&self`).
    viewport_size: Cell<(f32, f32)>,
    /// Whether to render two pages side by side in paginated mode.
    dual_page: DualPageMode,
    /// Fraction of viewport height used for page content (0.5..=1.0).
    /// A value below 1.0 compensates for inaccurate height estimation.
    page_height_fraction: f32,
    /// Deferred block index for page restoration.  Set when reading progress
    /// is loaded but pagination hasn't run yet (viewport_size still 0×0).
    /// Consumed by `maybe_repaginate()` once a valid layout is available.
    pending_block_index: Option<usize>,
    /// Whether the chapter navigation sidebar is visible.
    show_sidebar: bool,
    /// Content column max width as a percentage of the default 800px (50..=150).
    content_width_pct: f32,
    /// Font family used for rendering EPUB content.
    font_family: FontFamily,
    /// Pre-computed display names for the font family dropdown.
    font_family_names: widget::combo_box::State<FontFamily>,
    /// Ordered nav entries from the EPUB TOC (with depth and fragment-preserving hrefs).
    nav_entries: Vec<NavEntry>,
    /// Base body font size in pixels (12–24, default 16).
    base_font_size: f32,
    /// Block index of the navigation target to highlight briefly after fragment navigation.
    /// Set to `Some(idx)` on arrival; cleared by the deferred `ClearHighlight` message.
    highlighted_block: Option<usize>,
    /// Whether the search bar is currently visible.
    search_visible: bool,
    /// Current search query (lowercased when used for matching).
    search_query: String,
    /// Block indices in the active chapter that contain the search query.
    search_matches: Vec<usize>,
    /// Index into `search_matches` indicating the currently focused match.
    search_current: usize,
    /// Widget ID for the search text input (used to focus it on open).
    search_input_id: widget::Id,
    /// Shared font system for shaped text measurement (Phase 4b).
    /// Wrapped in `RefCell` for interior mutability: shaping mutates the font system
    /// cache but is logically read-only from the viewer's perspective.
    font_system: RefCell<FontSystem>,
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

        let (saved_font_family, saved_font_size) = load_epub_font_prefs();

        let viewer = EpubViewer {
            fingerprint: fingerprint.clone(),
            document,
            epub_document: None, // Will be loaded when chapters are loaded
            file_path: file_path.clone(),
            title: String::new(),
            chapters: Vec::new(),
            active_chapter: 0,
            initial_chapter: None,
            scroll_y: 0.0,
            saved_position: None,
            modifiers: Modifiers::default(),
            show_raw_html: false,
            content_scroll_id: widget::Id::unique(),
            view_mode: ViewMode::default(),
            current_page: 0,
            pagination_cache: HashMap::new(),
            viewport_size: Cell::new((0.0, 0.0)),
            dual_page: DualPageMode::default(),
            page_height_fraction: 1.0,
            pending_block_index: None,
            show_sidebar: true,
            content_width_pct: 100.0,
            font_family: saved_font_family,
            font_family_names: widget::combo_box::State::new(FontFamily::all()),
            nav_entries: Vec::new(),
            base_font_size: saved_font_size,
            highlighted_block: None,
            search_visible: false,
            search_query: String::new(),
            search_matches: Vec::new(),
            search_current: 0,
            search_input_id: widget::Id::unique(),
            font_system: RefCell::new(FontSystem::new()),
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
                |(title, chapters, epub_doc)| {
                    cosmic::action::app(EpubViewerMessage::EpubLoaded(
                        title,
                        chapters,
                        CloneableEpubDocument::from(epub_doc),
                    ))
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
                    Ok(None) => ReadingPosition::default(),
                    Err(e) => {
                        tracing::warn!("failed to load reading progress: {e}");
                        ReadingPosition::default()
                    }
                }
            },
            |pos| cosmic::action::app(EpubViewerMessage::ReadingProgressLoaded(pos)),
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
        let cosmic_theme::Spacing {
            space_xxs, space_s, ..
        } = theme::active().cosmic().spacing;

        let chapter_info = if !self.chapters.is_empty() {
            format!("{} / {}", self.active_chapter + 1, self.chapters.len())
        } else {
            String::new()
        };

        let current_href = self
            .chapters
            .get(self.active_chapter)
            .map(|c| c.href.as_str())
            .unwrap_or("");

        let capacity = if self.nav_entries.is_empty() {
            self.chapters.len()
        } else {
            self.nav_entries.len()
        };
        let mut column = widget::column::with_capacity(capacity)
            .padding(space_xxs)
            .spacing(space_xxs);

        if self.nav_entries.is_empty() {
            for (idx, chapter) in self.chapters.iter().enumerate() {
                let label = widget::text::body(&chapter.label)
                    .wrapping(cosmic::iced::widget::text::Wrapping::None);
                let button = widget::button::custom(label)
                    .on_press(EpubViewerMessage::SelectChapter(idx))
                    .selected(idx == self.active_chapter)
                    .width(Length::Fill);
                column = column.push(button);
            }
        } else {
            for (idx, entry) in self.nav_entries.iter().enumerate() {
                let base = entry
                    .href
                    .split_once('#')
                    .map(|(b, _)| b)
                    .unwrap_or(&entry.href);
                let selected = base == current_href;
                let label = widget::text::body(&entry.label)
                    .wrapping(cosmic::iced::widget::text::Wrapping::None);
                let button = widget::button::custom(label)
                    .on_press(EpubViewerMessage::SelectNavEntry(idx))
                    .selected(selected)
                    .width(Length::Fill);
                let indent = (entry.depth as f32) * (space_s as f32);
                let row = widget::row()
                    .push(widget::Space::with_width(Length::Fixed(indent)))
                    .push(button);
                column = column.push(row);
            }
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

    fn view_search_bar(&self) -> Element<'_, EpubViewerMessage> {
        let cosmic_theme::Spacing {
            space_xxs, space_s, ..
        } = theme::active().cosmic().spacing;

        let input = widget::text_input(fl!("epub-viewer-search-placeholder"), &self.search_query)
            .id(self.search_input_id.clone())
            .on_input(EpubViewerMessage::SetSearchQuery)
            .on_submit(|_| EpubViewerMessage::SearchNext)
            .width(Length::Fill);

        let match_label: Element<'_, EpubViewerMessage> = if !self.search_query.is_empty() {
            if self.search_matches.is_empty() {
                widget::text::body(fl!("epub-viewer-search-no-matches"))
                    .width(Length::Shrink)
                    .into()
            } else {
                let current = self.search_current + 1;
                widget::text::body(fl!(
                    "epub-viewer-search-match-count",
                    current = current,
                    total = self.search_matches.len()
                ))
                .width(Length::Shrink)
                .into()
            }
        } else {
            widget::Space::with_width(Length::Shrink).into()
        };

        widget::container(
            widget::row()
                .push(input)
                .push(match_label)
                .push(
                    widget::button::icon(widget::icon::from_name("go-up-symbolic").size(ICON_SIZE))
                        .on_press(EpubViewerMessage::SearchPrevious)
                        .tooltip(fl!("epub-viewer-search-prev"))
                        .padding(space_xxs),
                )
                .push(
                    widget::button::icon(
                        widget::icon::from_name("go-down-symbolic").size(ICON_SIZE),
                    )
                    .on_press(EpubViewerMessage::SearchNext)
                    .tooltip(fl!("epub-viewer-search-next"))
                    .padding(space_xxs),
                )
                .push(
                    widget::button::icon(
                        widget::icon::from_name("window-close-symbolic").size(ICON_SIZE),
                    )
                    .on_press(EpubViewerMessage::CloseSearch)
                    .tooltip(fl!("epub-viewer-search-close"))
                    .padding(space_xxs),
                )
                .spacing(space_xxs)
                .align_y(Vertical::Center)
                .padding([space_xxs, space_s]),
        )
        .class(Container::Secondary)
        .width(Length::Fill)
        .into()
    }

    fn view_content(&self) -> Element<'_, EpubViewerMessage> {
        match self.view_mode {
            ViewMode::Scroll => self.view_content_scroll(),
            ViewMode::Paginated => self.view_content_paginated(),
        }
    }

    fn view_content_scroll(&self) -> Element<'_, EpubViewerMessage> {
        let cosmic_theme::Spacing {
            space_s, space_xxs, ..
        } = theme::active().cosmic().spacing;

        if let Some(chapter) = self.chapters.get(self.active_chapter) {
            let mut column = widget::column::with_capacity(chapter.blocks.len())
                .spacing(space_xxs)
                .width(Length::Fill);

            if self.show_raw_html {
                column = column
                    .push(widget::text::monotext(chapter.raw_html.as_str()).width(Length::Fill));
            } else {
                let family = self.font_family.to_family();
                for (idx, block) in chapter.blocks.iter().enumerate() {
                    let highlight = if self.highlighted_block == Some(idx) {
                        BlockHighlight::Current
                    } else if !self.search_matches.is_empty() && self.search_matches.contains(&idx)
                    {
                        BlockHighlight::SearchMatch
                    } else {
                        BlockHighlight::None
                    };
                    column = column.push(render_block(
                        block,
                        highlight,
                        self.base_font_size,
                        family,
                        &self.document,
                        &chapter.href,
                        self.epub_document
                            .as_ref()
                            .map(CloneableEpubDocument::as_ref),
                    ));
                }
            }

            // Inner "paper" container with max-width for readability.
            // The inner container uses Fill + max_width so it expands up to
            // max_w but no further; the outer paper Shrinks to wrap it.
            let max_w = 800.0 * (self.content_width_pct / 100.0);
            let paper = widget::container(
                widget::container(column)
                    .padding(space_s)
                    .max_width(max_w)
                    .width(Length::Fill),
            )
            .style(move |theme: &cosmic::Theme| paper_background(theme))
            .width(Length::Shrink);

            // Outer "desk" container
            let outer = widget::container(paper)
                .style(desk_background)
                .width(Length::Fill)
                .align_x(Horizontal::Center);

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

    fn view_content_paginated(&self) -> Element<'_, EpubViewerMessage> {
        let cosmic_theme::Spacing {
            space_s, space_xxs, ..
        } = theme::active().cosmic().spacing;

        let chapter = match self.chapters.get(self.active_chapter) {
            Some(ch) => ch,
            None => return widget::Space::new(Length::Fill, Length::Fill).into(),
        };

        let current_page = self.current_page;
        let active_chapter = self.active_chapter;
        let show_raw_html = self.show_raw_html;

        let page_height_fraction = self.page_height_fraction;
        let max_content_width = 800.0 * (self.content_width_pct / 100.0);
        let base_font_size = self.base_font_size;

        widget::responsive(move |size| {
            // Store viewport size so update() can trigger re-pagination.
            self.viewport_size.set((size.width, size.height));

            // In raw HTML mode, fall back to a simple scrollable view.
            if show_raw_html {
                return widget::container(
                    widget::scrollable(
                        widget::text::monotext(chapter.raw_html.as_str()).width(Length::Fill),
                    )
                    .width(Length::Fill)
                    .height(Length::Fill),
                )
                .style(|theme: &cosmic::Theme| desk_background(theme))
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
            }

            let dual = self.should_dual_page(size.width);

            // Use cached layout if available, otherwise compute on-the-fly so
            // the first render after switching to paginated mode isn't empty.
            let cached_layout = self.pagination_cache.get(&active_chapter);
            let computed_layout;
            let layout = match cached_layout {
                Some(l) => l,
                None => {
                    let sp_s = theme::active().cosmic().spacing.space_s as f32;
                    let sp_xxs = theme::active().cosmic().spacing.space_xxs as f32;
                    let pw = if dual {
                        ((size.width - sp_xxs) / 2.0 - sp_s * 2.0).min(max_content_width)
                    } else {
                        max_content_width.min(size.width - sp_s * 2.0)
                    };
                    let ph = (size.height - sp_s * 2.0 - 24.0) * page_height_fraction;
                    computed_layout = paginate_blocks(
                        &chapter.blocks,
                        ph,
                        pw,
                        base_font_size,
                        &mut self.font_system.borrow_mut(),
                    );
                    &computed_layout
                }
            };
            let total = layout.pages.len();
            // Resolve deferred block index to the correct page.  The
            // pending value is consumed by `maybe_repaginate()` on the next
            // `update()`, but the viewport isn't available until this first
            // render, so we peek at it here to show the right page
            // immediately.
            let current_page = match self.pending_block_index {
                Some(block_idx) => layout
                    .pages
                    .iter()
                    .position(|p| p.start <= block_idx && block_idx < p.end)
                    .unwrap_or(0),
                None => current_page,
            };
            // Clamp page index to valid range (may be stale before update runs).
            let current_page = current_page.min(total.saturating_sub(1));

            // Build a single page "paper" element from a page index.
            let family = self.font_family.to_family();
            let make_paper = |page_idx: usize| -> Element<'_, EpubViewerMessage> {
                let page_range = layout.pages.get(page_idx);

                let mut column = widget::column().spacing(space_xxs).width(Length::Fill);
                if let Some(range) = page_range {
                    for (i, block) in chapter.blocks[range.start..range.end].iter().enumerate() {
                        let abs_idx = range.start + i;
                        let highlight = if self.highlighted_block == Some(abs_idx) {
                            BlockHighlight::Current
                        } else if !self.search_matches.is_empty()
                            && self.search_matches.contains(&abs_idx)
                        {
                            BlockHighlight::SearchMatch
                        } else {
                            BlockHighlight::None
                        };
                        column = column.push(render_block(
                            block,
                            highlight,
                            base_font_size,
                            family,
                            &self.document,
                            &chapter.href,
                            self.epub_document
                                .as_ref()
                                .map(CloneableEpubDocument::as_ref),
                        ));
                    }
                }

                let page_indicator =
                    widget::text::caption(format!("{} / {}", page_idx + 1, total,))
                        .width(Length::Fill)
                        .align_x(Horizontal::Center);

                // Wrap blocks in a scrollable so content that overflows the
                // estimated page height is still accessible.
                // The inner container uses max_width to constrain the text
                // column, and a centering wrapper keeps it horizontally centered
                // within the paper.
                let paper_content = widget::column()
                    .push(
                        widget::scrollable(
                            widget::container(
                                widget::container(column)
                                    .padding(space_s)
                                    .max_width(max_content_width)
                                    .width(Length::Shrink),
                            )
                            .width(Length::Fill)
                            .align_x(Horizontal::Center),
                        )
                        .height(Length::Fill),
                    )
                    .push(page_indicator)
                    .height(Length::Fill);

                widget::container(paper_content)
                    .style(move |theme: &cosmic::Theme| paper_background(theme))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            };

            // Build the center content: single or dual page.
            let center_content: Element<'_, EpubViewerMessage> = if dual {
                let left_paper = make_paper(current_page);
                let right_paper = if current_page + 1 < total {
                    make_paper(current_page + 1)
                } else {
                    // Empty right page when on the last page.
                    widget::container(widget::Space::new(Length::Fill, Length::Fill))
                        .style(move |theme: &cosmic::Theme| paper_background(theme))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .into()
                };
                widget::row()
                    .push(left_paper)
                    .spacing(space_xxs)
                    .push(right_paper)
                    .height(Length::Fill)
                    .into()
            } else {
                make_paper(current_page)
            };

            // Outer "desk" with click-to-turn zones.
            let left_zone =
                widget::mouse_area(widget::Space::new(Length::FillPortion(1), Length::Fill))
                    .on_press(EpubViewerMessage::PreviousPage);

            let center = widget::container(center_content)
                .width(Length::FillPortion(8))
                .height(Length::Fill);

            let right_zone =
                widget::mouse_area(widget::Space::new(Length::FillPortion(1), Length::Fill))
                    .on_press(EpubViewerMessage::NextPage);

            let outer = widget::container(
                widget::row()
                    .push(left_zone)
                    .push(center)
                    .push(right_zone)
                    .height(Length::Fill),
            )
            .style(desk_background)
            .width(Length::Fill)
            .height(Length::Fill);

            outer.into()
        })
        .into()
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

            return no_source.apply(full_page);
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

            return loading.apply(full_page);
        }

        let main_col = widget::column()
            .height(Length::Fill)
            .push_maybe(self.search_visible.then(|| self.view_search_bar()))
            .push(self.view_content());

        widget::row()
            .height(Length::Fill)
            .push_maybe(self.show_sidebar.then(|| self.view_chapter_sidebar()))
            .push(main_col)
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
            widget::button::icon(widget::icon::from_name("system-search-symbolic").size(ICON_SIZE))
                .on_press(EpubViewerMessage::ToggleSearch)
                .tooltip(fl!("epub-viewer-search"))
                .padding(space_xxs)
                .into(),
            widget::button::icon(widget::icon::from_name("window-close-symbolic").size(ICON_SIZE))
                .on_press(EpubViewerMessage::Out(EpubViewerOutput::Close(
                    self.fingerprint.clone(),
                    if self.chapters.is_empty() {
                        None
                    } else {
                        let first_block = self
                            .pagination_cache
                            .get(&self.active_chapter)
                            .and_then(|l| l.pages.get(self.current_page))
                            .map(|p| p.start)
                            .unwrap_or(0);
                        Some(serialize_progress(
                            self.active_chapter,
                            self.scroll_y,
                            first_block,
                            self.view_mode,
                        ))
                    },
                )))
                .tooltip(fl!("epub-viewer-back"))
                .padding(space_xxs)
                .into(),
        ]
    }

    fn view_context(&self) -> ContextView<'_, EpubViewerMessage> {
        let mut display_section = widget::settings::section().title(fl!("epub-viewer-display"));

        display_section = display_section.add(
            widget::settings::item::builder(fl!("epub-viewer-show-sidebar"))
                .toggler(self.show_sidebar, EpubViewerMessage::ShowSidebar),
        );

        display_section = display_section.add(
            widget::settings::item::builder(fl!("epub-viewer-view-paginated")).toggler(
                self.view_mode == ViewMode::Paginated,
                |enabled| {
                    EpubViewerMessage::SetViewMode(if enabled {
                        ViewMode::Paginated
                    } else {
                        ViewMode::Scroll
                    })
                },
            ),
        );

        if self.view_mode == ViewMode::Paginated {
            display_section = display_section.add(
                widget::settings::item::builder(fl!("epub-viewer-dual-page")).control(
                    widget::settings::item_row(vec![
                        widget::radio(
                            widget::text::body(fl!("epub-viewer-dual-page-off")),
                            DualPageMode::Off,
                            Some(self.dual_page),
                            EpubViewerMessage::SetDualPage,
                        )
                        .into(),
                        widget::radio(
                            widget::text::body(fl!("epub-viewer-dual-page-auto")),
                            DualPageMode::Auto,
                            Some(self.dual_page),
                            EpubViewerMessage::SetDualPage,
                        )
                        .into(),
                        widget::radio(
                            widget::text::body(fl!("epub-viewer-dual-page-on")),
                            DualPageMode::On,
                            Some(self.dual_page),
                            EpubViewerMessage::SetDualPage,
                        )
                        .into(),
                    ]),
                ),
            );

            let pct = (self.page_height_fraction * 100.0).round() as u32;
            display_section = display_section.add(
                widget::settings::item::builder(format!(
                    "{} ({}%)",
                    fl!("epub-viewer-page-fill"),
                    pct
                ))
                .control(
                    widget::slider(50.0..=100.0, self.page_height_fraction * 100.0, |v| {
                        EpubViewerMessage::SetPageHeightFraction(v / 100.0)
                    })
                    .step(5.0),
                ),
            );
        }

        let width_pct = self.content_width_pct.round() as u32;
        display_section = display_section.add(
            widget::settings::item::builder(format!(
                "{} ({}%)",
                fl!("epub-viewer-content-width"),
                width_pct
            ))
            .control(
                widget::slider(50.0..=150.0, self.content_width_pct, |v| {
                    EpubViewerMessage::SetContentMaxWidth(v)
                })
                .step(5.0),
            ),
        );

        display_section = display_section.add(
            widget::settings::item::builder(fl!("epub-viewer-font")).control(widget::combo_box(
                &self.font_family_names,
                &fl!("epub-viewer-font"),
                Some(&self.font_family),
                EpubViewerMessage::SetFontFamily,
            )),
        );

        let font_size_px = self.base_font_size.round() as u32;
        display_section = display_section.add(
            widget::settings::item::builder(format!(
                "{} ({}px)",
                fl!("epub-viewer-font-size"),
                font_size_px
            ))
            .control(
                widget::slider(12.0..=24.0, self.base_font_size, |v| {
                    EpubViewerMessage::SetBaseFontSize(v)
                })
                .step(1.0),
            ),
        );

        let display_section = display_section.add(
            widget::settings::item::builder(fl!("epub-viewer-raw-html"))
                .toggler(self.show_raw_html, EpubViewerMessage::ShowRawHtml),
        );

        let shortcuts_section = if self.view_mode == ViewMode::Paginated {
            widget::settings::section()
                .title(fl!("epub-viewer-keyboard-shortcuts"))
                .add(shortcut_item(
                    "← PgUp",
                    fl!("epub-viewer-shortcut-previous-page"),
                ))
                .add(shortcut_item(
                    "→ PgDn",
                    fl!("epub-viewer-shortcut-next-page"),
                ))
        } else {
            widget::settings::section()
                .title(fl!("epub-viewer-keyboard-shortcuts"))
                .add(shortcut_item(
                    "↑ ← PgUp",
                    fl!("epub-viewer-shortcut-previous-chapter"),
                ))
                .add(shortcut_item(
                    "↓ → PgDn",
                    fl!("epub-viewer-shortcut-next-chapter"),
                ))
        };

        ContextView {
            title: fl!("epub-viewer"),
            content: widget::settings::view_column(vec![
                display_section.into(),
                shortcuts_section.into(),
            ])
            .into(),
        }
    }

    fn update(&mut self, message: EpubViewerMessage) -> Task<Action<EpubViewerMessage>> {
        self.maybe_repaginate();
        match message {
            EpubViewerMessage::EpubLoaded(title, chapters, epub_doc) => {
                self.title = title;
                self.chapters = chapters;
                self.nav_entries = epub_doc.as_ref().nav().to_vec();
                self.epub_document = Some(epub_doc);
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
            EpubViewerMessage::ReadingProgressLoaded(pos) => {
                self.initial_chapter = pos.chapter;
                self.scroll_y = pos.scroll_y;
                if !self.chapters.is_empty()
                    && let Some(c) = pos.chapter
                    && c < self.chapters.len()
                {
                    self.active_chapter = c;
                }
                // Restore view mode if it was persisted.
                if let Some(mode) = pos.view_mode {
                    self.view_mode = mode;
                }
                // Store block index for deferred page restoration.
                // Pagination can't run yet (viewport_size is 0×0 until the
                // first render), so `maybe_repaginate()` will consume this
                // once a valid layout is available.
                self.pending_block_index = pos.block_index;
                // Restore scroll if chapters are already loaded (scroll mode)
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
                    self.current_page = 0;
                    self.saved_position = None;
                    self.highlighted_block = None;
                    self.search_matches.clear();
                    self.search_current = 0;
                }
                Task::none()
            }
            EpubViewerMessage::SelectNavEntry(nav_idx) => {
                if let Some(entry) = self.nav_entries.get(nav_idx) {
                    let (base, fragment) = match entry.href.split_once('#') {
                        Some((b, f)) => (b, Some(f)),
                        None => (entry.href.as_str(), None),
                    };
                    if let Some(idx) = self.chapters.iter().position(|c| c.href == base) {
                        self.active_chapter = idx;
                        self.scroll_y = 0.0;
                        self.current_page = 0;
                        self.saved_position = None;
                        self.highlighted_block = None;
                        self.search_matches.clear();
                        self.search_current = 0;

                        // Navigate to the fragment position within the chapter.
                        if let Some(frag) = fragment.filter(|f| !f.is_empty()) {
                            let chapter = &self.chapters[idx];

                            // Find the Anchor block index — used for both highlighting
                            // and paginated navigation.
                            let block_idx = chapter.blocks.iter().position(
                                |b| matches!(b, ContentBlock::Anchor { id } if id == frag),
                            );

                            let mut nav_task = Task::none();

                            match self.view_mode {
                                ViewMode::Scroll => {
                                    if let Some(&target_y) = chapter.anchors.get(frag) {
                                        self.scroll_y = target_y;
                                        nav_task = scrollable::scroll_to(
                                            self.content_scroll_id.clone(),
                                            scrollable::AbsoluteOffset {
                                                x: 0.0,
                                                y: target_y,
                                            },
                                        );
                                    }
                                }
                                ViewMode::Paginated => {
                                    // Find the Anchor block with this id to determine
                                    // which page to navigate to.
                                    if let Some(bi) = block_idx {
                                        if let Some(layout) = self.pagination_cache.get(&idx) {
                                            self.current_page = layout
                                                .pages
                                                .iter()
                                                .position(|p| p.start <= bi && bi < p.end)
                                                .unwrap_or(0);
                                        } else {
                                            self.pending_block_index = Some(bi);
                                        }
                                    }
                                }
                            }

                            // Set highlight on the block immediately following the Anchor
                            // and schedule a deferred clear.
                            if let Some(bi) = block_idx
                                && bi + 1 < chapter.blocks.len()
                            {
                                self.highlighted_block = Some(bi + 1);
                                let clear_task = Task::perform(
                                    async {
                                        tokio::time::sleep(std::time::Duration::from_millis(1500))
                                            .await;
                                    },
                                    |_| cosmic::action::app(EpubViewerMessage::ClearHighlight),
                                );
                                return Task::batch([nav_task, clear_task]);
                            }

                            return nav_task;
                        }
                    }
                }
                Task::none()
            }
            EpubViewerMessage::ShowRawHtml(show) => {
                self.show_raw_html = show;
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
                    if let Some(frag) = fragment.filter(|f| !f.is_empty())
                        && let Some(chapter) = self.chapters.get(self.active_chapter)
                    {
                        if let Some(&target_y) = chapter.anchors.get(frag) {
                            // Navigating to a footnote: save reading position (once).
                            if self.saved_position.is_none() {
                                self.saved_position = Some(match self.view_mode {
                                    ViewMode::Scroll => SavedPosition::ScrollY(self.scroll_y),
                                    ViewMode::Paginated => {
                                        SavedPosition::PageIndex(self.current_page)
                                    }
                                });
                            }
                            if self.view_mode == ViewMode::Paginated {
                                // Find page containing the footnote block.
                                if let Some(block_idx) = chapter.blocks.iter().position(|b| {
                                    matches!(b, ContentBlock::Footnote { id, .. } if id == frag)
                                })
                                    && let Some(layout) =
                                        self.pagination_cache.get(&self.active_chapter)
                                        && let Some(page_idx) = layout.pages.iter().position(
                                            |p| p.start <= block_idx && block_idx < p.end,
                                        ) {
                                            self.current_page = page_idx;
                                        }
                                return Task::none();
                            }
                            return scrollable::scroll_to(
                                self.content_scroll_id.clone(),
                                scrollable::AbsoluteOffset {
                                    x: 0.0,
                                    y: target_y,
                                },
                            );
                        } else if let Some(saved) = self.saved_position.take() {
                            // Unknown fragment (likely a back-reference ↩): restore
                            // the position saved before the last footnote jump.
                            match saved {
                                SavedPosition::PageIndex(page) => {
                                    self.current_page = page;
                                    return Task::none();
                                }
                                SavedPosition::ScrollY(y) => {
                                    return scrollable::scroll_to(
                                        self.content_scroll_id.clone(),
                                        scrollable::AbsoluteOffset { x: 0.0, y },
                                    );
                                }
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
                        self.current_page = 0;
                        self.saved_position = None;
                    }
                }
                Task::none()
            }
            EpubViewerMessage::Key(modifiers, key, _text) => {
                // Ctrl+F: toggle search bar.
                if modifiers.control() && matches!(&key, Key::Character(c) if c.as_str() == "f") {
                    return self.update(EpubViewerMessage::ToggleSearch);
                }
                // Escape: close search if open, otherwise ignore.
                if matches!(&key, Key::Named(Named::Escape)) && self.search_visible {
                    return self.update(EpubViewerMessage::CloseSearch);
                }
                match (&key, self.view_mode) {
                    (Key::Named(Named::ArrowLeft | Named::PageUp), ViewMode::Paginated) => {
                        self.update(EpubViewerMessage::PreviousPage)
                    }
                    (Key::Named(Named::ArrowRight | Named::PageDown), ViewMode::Paginated) => {
                        self.update(EpubViewerMessage::NextPage)
                    }
                    (
                        Key::Named(Named::ArrowUp | Named::ArrowLeft | Named::PageUp),
                        ViewMode::Scroll,
                    ) => {
                        if self.active_chapter > 0 {
                            self.active_chapter -= 1;
                            self.scroll_y = 0.0;
                        }
                        Task::none()
                    }
                    (
                        Key::Named(Named::ArrowDown | Named::ArrowRight | Named::PageDown),
                        ViewMode::Scroll,
                    ) => {
                        if self.active_chapter + 1 < self.chapters.len() {
                            self.active_chapter += 1;
                            self.scroll_y = 0.0;
                        }
                        Task::none()
                    }
                    _ => Task::none(),
                }
            }
            EpubViewerMessage::SetViewMode(mode) => {
                self.view_mode = mode;
                if mode == ViewMode::Paginated {
                    self.maybe_repaginate();
                    if self.pagination_cache.contains_key(&self.active_chapter) {
                        // Cache available — map scroll position to page now.
                        self.current_page = self.scroll_y_to_page();
                    } else if self.scroll_y > 0.0 {
                        // No cache yet (viewport_size unknown).  Find the
                        // approximate first-visible block from scroll_y and
                        // defer page restoration to maybe_repaginate().
                        self.pending_block_index =
                            self.approximate_block_at_scroll_y(self.scroll_y);
                    }
                } else {
                    self.scroll_y = self.page_to_scroll_y();
                }
                Task::none()
            }
            EpubViewerMessage::NextPage => {
                self.maybe_repaginate();
                let total = self.total_pages();
                let step = self.page_step();
                if self.current_page + step < total {
                    self.current_page += step;
                } else if self.current_page + 1 < total {
                    // Partial step: go to last page.
                    self.current_page = total - 1;
                } else if self.active_chapter + 1 < self.chapters.len() {
                    self.active_chapter += 1;
                    self.current_page = 0;
                    self.maybe_repaginate();
                }
                Task::none()
            }
            EpubViewerMessage::PreviousPage => {
                self.maybe_repaginate();
                let step = self.page_step();
                if self.current_page >= step {
                    self.current_page -= step;
                } else if self.current_page > 0 {
                    self.current_page = 0;
                } else if self.active_chapter > 0 {
                    self.active_chapter -= 1;
                    self.maybe_repaginate();
                    self.current_page = self.total_pages().saturating_sub(1);
                }
                Task::none()
            }
            EpubViewerMessage::SetDualPage(mode) => {
                self.dual_page = mode;
                // Invalidate pagination cache since page width changes.
                self.pagination_cache.clear();
                self.maybe_repaginate();
                Task::none()
            }
            EpubViewerMessage::SetPageHeightFraction(frac) => {
                self.page_height_fraction = frac.clamp(0.5, 1.0);
                self.pagination_cache.clear();
                self.maybe_repaginate();
                Task::none()
            }
            EpubViewerMessage::ShowSidebar(show) => {
                self.show_sidebar = show;
                Task::none()
            }
            EpubViewerMessage::SetContentMaxWidth(pct) => {
                self.content_width_pct = pct.clamp(50.0, 150.0);
                self.pagination_cache.clear();
                self.maybe_repaginate();
                Task::none()
            }
            EpubViewerMessage::SetFontFamily(family) => {
                self.font_family = family;
                self.pagination_cache.clear();
                self.maybe_repaginate();
                save_epub_font_prefs(self.font_family, self.base_font_size);
                Task::none()
            }
            EpubViewerMessage::SetBaseFontSize(size) => {
                self.base_font_size = size.clamp(12.0, 24.0);
                self.pagination_cache.clear();
                self.maybe_repaginate();
                save_epub_font_prefs(self.font_family, self.base_font_size);
                Task::none()
            }
            EpubViewerMessage::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers;
                Task::none()
            }
            EpubViewerMessage::ClearHighlight => {
                self.highlighted_block = None;
                Task::none()
            }
            EpubViewerMessage::ToggleSearch => {
                self.search_visible = !self.search_visible;
                if self.search_visible {
                    widget::text_input::focus(self.search_input_id.clone())
                } else {
                    self.search_query.clear();
                    self.search_matches.clear();
                    self.search_current = 0;
                    self.highlighted_block = None;
                    Task::none()
                }
            }
            EpubViewerMessage::SetSearchQuery(query) => {
                self.search_query = query;
                self.highlighted_block = None;
                if self.search_query.is_empty() {
                    self.search_matches.clear();
                    self.search_current = 0;
                    return Task::none();
                }
                let lower = self.search_query.to_lowercase();
                if let Some(chapter) = self.chapters.get(self.active_chapter) {
                    self.search_matches = chapter
                        .blocks
                        .iter()
                        .enumerate()
                        .filter(|(_, b)| block_contains(b, &lower))
                        .map(|(i, _)| i)
                        .collect();
                }
                self.search_current = 0;
                self.navigate_to_search_match()
            }
            EpubViewerMessage::SearchNext => {
                if !self.search_matches.is_empty() {
                    self.search_current = (self.search_current + 1) % self.search_matches.len();
                    return self.navigate_to_search_match();
                }
                Task::none()
            }
            EpubViewerMessage::SearchPrevious => {
                if !self.search_matches.is_empty() {
                    self.search_current = if self.search_current == 0 {
                        self.search_matches.len() - 1
                    } else {
                        self.search_current - 1
                    };
                    return self.navigate_to_search_match();
                }
                Task::none()
            }
            EpubViewerMessage::CloseSearch => {
                self.search_visible = false;
                self.search_query.clear();
                self.search_matches.clear();
                self.search_current = 0;
                self.highlighted_block = None;
                Task::none()
            }
            EpubViewerMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent component")
            }
        }
    }
}

fn paper_background(theme: &cosmic::Theme) -> widget::container::Style {
    let c = theme.cosmic().bg_color();
    widget::container::background(cosmic::iced::Color::from_rgba(
        c.color.red,
        c.color.green,
        c.color.blue,
        c.alpha,
    ))
}

fn desk_background(theme: &cosmic::Theme) -> widget::container::Style {
    let c = theme.cosmic().bg_component_color();
    widget::container::background(cosmic::iced::Color::from_rgba(
        c.color.red,
        c.color.green,
        c.color.blue,
        c.alpha,
    ))
}

fn highlight_background(theme: &cosmic::Theme) -> cosmic::iced::Color {
    let accent = theme.cosmic().accent.base;
    cosmic::iced::Color::from_rgba(
        accent.color.red,
        accent.color.green,
        accent.color.blue,
        accent.alpha,
    )
}

fn highlight_text_color(theme: &cosmic::Theme) -> cosmic::iced::Color {
    let accent = theme.cosmic().accent.on;
    cosmic::iced::Color::from_rgba(
        accent.color.red,
        accent.color.green,
        accent.color.blue,
        accent.alpha,
    )
}

fn search_match_background(theme: &cosmic::Theme) -> cosmic::iced::Color {
    let accent = theme.cosmic().accent.base;
    cosmic::iced::Color::from_rgba(accent.color.red, accent.color.green, accent.color.blue, 0.2)
}

/// Return true if `block` contains `query` (lowercased) anywhere in its text.
fn block_contains(block: &ContentBlock, query: &str) -> bool {
    match block {
        ContentBlock::Heading { text, spans, .. }
        | ContentBlock::Paragraph { text, spans, .. }
        | ContentBlock::Preformatted { text, spans, .. } => {
            if !spans.is_empty() {
                spans.iter().any(|s| s.text.to_lowercase().contains(query))
            } else {
                text.to_lowercase().contains(query)
            }
        }
        ContentBlock::UnorderedList { items } => items.iter().any(|item| {
            if !item.spans.is_empty() {
                item.spans
                    .iter()
                    .any(|s| s.text.to_lowercase().contains(query))
            } else {
                item.text.to_lowercase().contains(query)
            }
        }),
        ContentBlock::OrderedList { items, .. } => items.iter().any(|item| {
            if !item.spans.is_empty() {
                item.spans
                    .iter()
                    .any(|s| s.text.to_lowercase().contains(query))
            } else {
                item.text.to_lowercase().contains(query)
            }
        }),
        ContentBlock::Table { rows } => rows.iter().any(|row| {
            row.iter().any(|cell| {
                if !cell.spans.is_empty() {
                    cell.spans
                        .iter()
                        .any(|s| s.text.to_lowercase().contains(query))
                } else {
                    cell.text.to_lowercase().contains(query)
                }
            })
        }),
        ContentBlock::BlockQuote { children } => children.iter().any(|b| block_contains(b, query)),
        ContentBlock::Footnote { blocks, .. } => blocks.iter().any(|b| block_contains(b, query)),
        ContentBlock::Figure {
            blocks,
            caption_text,
            ..
        } => {
            caption_text.to_lowercase().contains(query)
                || blocks.iter().any(|b| block_contains(b, query))
        }
        ContentBlock::Image { alt, .. } | ContentBlock::Svg { alt, .. } => {
            alt.to_lowercase().contains(query)
        }
        ContentBlock::Anchor { .. } | ContentBlock::HorizontalRule => false,
    }
}

fn shortcut_item<'a>(key: &'a str, description: String) -> Element<'a, EpubViewerMessage> {
    widget::settings::item::builder(description)
        .control(widget::text::monotext(key))
        .into()
}

/// Serialize reading progress to a JSON string.
/// Includes scroll offset, block index, and view mode. The format is
/// backward-compatible: older versions silently ignore unknown fields.
fn serialize_progress(
    chapter: usize,
    scroll_y: f32,
    first_block: usize,
    view_mode: ViewMode,
) -> String {
    let mode = match view_mode {
        ViewMode::Scroll => "scroll",
        ViewMode::Paginated => "paginated",
    };
    format!(
        "{{\"chapter\":{chapter},\"scroll\":{scroll_y},\"block\":{first_block},\"mode\":\"{mode}\"}}"
    )
}

/// Parsed reading position from a progress JSON string.
#[derive(Clone, Debug, Default)]
pub(crate) struct ReadingPosition {
    chapter: Option<usize>,
    scroll_y: f32,
    /// Index of the first visible block (for paginated mode restoration).
    block_index: Option<usize>,
    /// View mode that was active when progress was saved.
    view_mode: Option<ViewMode>,
}

/// Parse reading progress from a JSON string like
/// `{"chapter":2,"scroll":340.5,"block":15}`.
fn parse_reading_progress(progress: &str) -> ReadingPosition {
    let mut pos = ReadingPosition::default();
    let progress = progress.trim();
    if let Some(inner) = progress.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        for part in inner.split(',') {
            if let Some((key, value)) = part.split_once(':') {
                match key.trim().trim_matches('"') {
                    "chapter" => pos.chapter = value.trim().parse().ok(),
                    "scroll" => pos.scroll_y = value.trim().parse().unwrap_or(0.0),
                    "block" => pos.block_index = value.trim().parse().ok(),
                    "mode" => {
                        pos.view_mode = match value.trim().trim_matches('"') {
                            "paginated" => Some(ViewMode::Paginated),
                            "scroll" => Some(ViewMode::Scroll),
                            _ => None,
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    pos
}

/// Load EPUB chapters from a file path. Runs on a blocking thread.
fn load_epub_chapters(path: &Path) -> (String, Vec<EpubChapter>, EpubDocument) {
    let epub_doc = match EpubDocument::open(path) {
        Ok(doc) => doc,
        Err(e) => {
            tracing::error!("failed to open EPUB: {e}");
            // Return empty chapters and a dummy document
            let dummy_doc = EpubDocument::open(path).unwrap_or_else(|_| {
                // Create a minimal document by trying again or panic
                EpubDocument::open(path).expect("Failed to create EPUB document")
            });
            return (String::new(), Vec::new(), dummy_doc);
        }
    };

    let title = epub_doc.metadata().title.clone().unwrap_or_default();

    let spine = epub_doc.spine().to_vec();
    let mut chapters = Vec::with_capacity(spine.len());

    for (idx, item) in spine.iter().filter(|item| item.linear).enumerate() {
        let label = item
            .label
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| item.id.clone())
            .apply_when(String::is_empty, |_| format!("Chapter {}", idx + 1));

        let (blocks, raw_html) = match epub_doc.resolve_resource(&item.href) {
            Ok(data) => {
                let href = &item.href;
                let raw = String::from_utf8_lossy(&data).into_owned();
                let stylesheet = load_chapter_stylesheets(&raw, href, &epub_doc);
                let blocks =
                    epub::content::parse_xhtml(&data, href, &stylesheet, &mut |img_path| {
                        match epub_doc.resolve_resource(img_path) {
                            Ok(img_data) => {
                                let media_type = epub::content::guess_media_type(img_path);
                                Some((img_data, media_type))
                            }
                            Err(_) => None,
                        }
                    });
                (blocks, raw)
            }
            Err(e) => {
                tracing::warn!("failed to resolve spine item {}: {e}", item.href);
                (Vec::new(), String::new())
            }
        };

        let anchors = build_anchor_map(&blocks);
        chapters.push(EpubChapter {
            label,
            href: item.href.clone(),
            blocks,
            anchors,
            raw_html,
        });
    }

    (title, chapters, epub_doc)
}

/// Extract `<link rel="stylesheet" href="...">` references from XHTML and load
/// the stylesheets from the EPUB archive. Returns a merged `StyleSheet`.
fn load_chapter_stylesheets(xhtml: &str, chapter_href: &str, doc: &EpubDocument) -> StyleSheet {
    let mut stylesheet = StyleSheet::empty();
    let base = epub::content::base_dir(chapter_href);

    // Simple scan for <link> tags in the <head> — no need for a full parser.
    // Look for patterns like: <link rel="stylesheet" href="..." />
    for segment in xhtml.split("<link") {
        // Must be in a tag context (i.e. followed by attributes and '>'),
        // and must have rel="stylesheet".
        let Some(end) = segment.find('>') else {
            continue;
        };
        let tag_content = &segment[..end];
        let lower = tag_content.to_ascii_lowercase();
        if !lower.contains("stylesheet") {
            continue;
        }
        // Extract href value
        let href = extract_attr_value(tag_content, "href");
        let Some(href) = href else { continue };

        let resolved = epub::content::resolve_href(base, href);
        match doc.resolve_resource(&resolved) {
            Ok(css_data) => {
                let css_text = String::from_utf8_lossy(&css_data);
                let sheet = epub::content::parse_css(&css_text);
                stylesheet.merge(sheet);
            }
            Err(e) => {
                tracing::debug!("failed to load stylesheet {resolved}: {e}");
            }
        }
    }

    stylesheet
}

/// Extract the value of an HTML attribute from a tag fragment.
/// Handles both single and double-quoted values.
fn extract_attr_value<'a>(tag_content: &'a str, attr_name: &str) -> Option<&'a str> {
    // Find the attribute name (case-insensitive)
    let lower = tag_content.to_ascii_lowercase();
    let attr_pos = lower.find(attr_name)?;
    let rest = &tag_content[attr_pos + attr_name.len()..];
    // Skip whitespace and '='
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('=')?;
    let rest = rest.trim_start();
    // Extract quoted value
    if let Some(rest) = rest.strip_prefix('"') {
        let end = rest.find('"')?;
        Some(&rest[..end])
    } else if let Some(rest) = rest.strip_prefix('\'') {
        let end = rest.find('\'')?;
        Some(&rest[..end])
    } else {
        // Unquoted value (up to whitespace or >)
        let end = rest
            .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .unwrap_or(rest.len());
        if end > 0 { Some(&rest[..end]) } else { None }
    }
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
        ContentBlock::Svg { .. } => 200.0,
        ContentBlock::Table { rows } => rows.len() as f32 * 36.0 + 8.0,
        ContentBlock::HorizontalRule => 16.0,
        ContentBlock::Footnote { blocks, .. } => {
            blocks.iter().map(estimated_block_height).sum::<f32>() + 16.0
        }
        ContentBlock::Figure {
            blocks,
            caption_text,
            ..
        } => {
            let blocks_h: f32 = blocks.iter().map(estimated_block_height).sum::<f32>();
            let caption_h = if caption_text.is_empty() { 0.0 } else { 22.0 };
            blocks_h + caption_h + 8.0
        }
        ContentBlock::Anchor { .. } => 0.0,
    }
}

/// Build a map of HTML anchor id → estimated absolute y-offset (pixels).
/// Covers `ContentBlock::Anchor` ids (headings, sections, etc.) and
/// `ContentBlock::Footnote` ids.
fn build_anchor_map(blocks: &[ContentBlock]) -> HashMap<String, f32> {
    const SPACING: f32 = 8.0;
    let mut map = HashMap::new();
    let mut y = 0.0f32;
    for block in blocks {
        match block {
            ContentBlock::Anchor { id } if !id.is_empty() => {
                map.insert(id.clone(), y);
            }
            ContentBlock::Footnote { id, .. } if !id.is_empty() => {
                map.entry(id.clone()).or_insert(y);
            }
            _ => {}
        }
        y += estimated_block_height(block) + SPACING;
    }
    map
}

/// Measure the pixel height that `text` occupies when shaped at `font_size` and wrapped to
/// `content_width`, using `cosmic-text` for accurate glyph metrics (Phase 4b).
fn measure_text_height(
    text: &str,
    content_width: f32,
    font_size: f32,
    font_system: &mut FontSystem,
) -> f32 {
    use cosmic_text::Attrs;
    use cosmic_text::Buffer;
    use cosmic_text::Metrics;
    use cosmic_text::Shaping;
    let line_height = font_size * 1.375;
    let metrics = Metrics::new(font_size, line_height);
    let mut buffer = Buffer::new(font_system, metrics);
    buffer.set_size(font_system, Some(content_width), None);
    buffer.set_text(font_system, text, &Attrs::new(), Shaping::Advanced, None);
    buffer.shape_until_scroll(font_system, false);
    buffer
        .layout_runs()
        .last()
        .map(|r| r.line_y + r.line_height)
        .unwrap_or(line_height)
}

/// Estimate the rendered height of a block given the available content width and font size.
/// Uses [`measure_text_height`] (cosmic-text shaping) for text blocks (Phase 4b), and
/// word-boundary line counting (Phase 4a) for list items and other block types.
fn estimated_block_height_for_width(
    block: &ContentBlock,
    content_width: f32,
    font_size: f32,
    font_system: &mut FontSystem,
) -> f32 {
    let scale = font_size / 16.0;
    let line_h = font_size * 1.375;
    match block {
        ContentBlock::Heading { level, text, .. } => {
            let heading_size = match level {
                1 => font_size * 2.0,
                2 => font_size * 1.75,
                3 => font_size * 1.5,
                4 => font_size * 1.25,
                _ => font_size * 1.125,
            };
            measure_text_height(text, content_width, heading_size, font_system)
                .max(heading_size * 1.375)
        }
        ContentBlock::Paragraph { text, .. } => {
            measure_text_height(text, content_width, font_size, font_system).max(line_h)
        }
        ContentBlock::Preformatted { text, .. } => {
            (text.lines().count() as f32 * line_h).max(line_h)
        }
        ContentBlock::BlockQuote { children } => {
            children
                .iter()
                .map(|b| {
                    estimated_block_height_for_width(
                        b,
                        content_width - 16.0,
                        font_size,
                        font_system,
                    )
                })
                .sum::<f32>()
                + 16.0 * scale
        }
        ContentBlock::UnorderedList { items } => items
            .iter()
            .map(|item| {
                // Subtract bullet prefix width (≈ 24 px)
                measure_text_height(
                    &item.text,
                    (content_width - 24.0).max(40.0),
                    font_size,
                    font_system,
                )
                .max(line_h)
            })
            .sum::<f32>(),
        ContentBlock::OrderedList { items, .. } => items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                // Subtract number prefix width (≈ 24 + digit_count * 8 px)
                let prefix_px = 24.0 + (i + 1).to_string().len() as f32 * 8.0;
                measure_text_height(
                    &item.text,
                    (content_width - prefix_px).max(40.0),
                    font_size,
                    font_system,
                )
                .max(line_h)
            })
            .sum::<f32>(),
        ContentBlock::Image {
            natural_width,
            natural_height,
            ..
        } => {
            if *natural_width > 0 && *natural_height > 0 {
                content_width * (*natural_height as f32 / *natural_width as f32)
            } else {
                200.0
            }
        }
        ContentBlock::Svg { aspect_ratio, .. } => {
            // aspect_ratio = width / height → height = content_width / aspect_ratio
            if let Some(ar) = aspect_ratio {
                if *ar > 0.0 { content_width / ar } else { 200.0 }
            } else {
                200.0
            }
        }
        ContentBlock::Table { rows } => rows.len() as f32 * (line_h * 2.0 + 4.0) + 8.0,
        ContentBlock::HorizontalRule => 16.0 * scale,
        ContentBlock::Footnote { blocks, .. } => {
            blocks
                .iter()
                .map(|b| {
                    estimated_block_height_for_width(
                        b,
                        content_width - 16.0,
                        font_size * 0.8,
                        font_system,
                    )
                })
                .sum::<f32>()
                + 16.0 * scale
        }
        ContentBlock::Figure {
            blocks,
            caption_text,
            ..
        } => {
            let caption_size = font_size * 0.85;
            let caption_h = if caption_text.is_empty() {
                0.0
            } else {
                measure_text_height(caption_text, content_width, caption_size, font_system)
            };
            let blocks_h: f32 = blocks
                .iter()
                .map(|b| estimated_block_height_for_width(b, content_width, font_size, font_system))
                .sum::<f32>();
            blocks_h + caption_h + 8.0 * scale
        }
        ContentBlock::Anchor { .. } => 0.0,
    }
}

/// Split a chapter's blocks into pages that fit within `page_height` pixels.
///
/// Blocks that individually exceed `page_height` get their own page (no
/// mid-block splitting).
fn paginate_blocks(
    blocks: &[ContentBlock],
    page_height: f32,
    page_width: f32,
    font_size: f32,
    font_system: &mut FontSystem,
) -> PaginationLayout {
    const SPACING: f32 = 8.0;

    let mut pages = Vec::new();
    let mut page_start = 0;
    let mut accumulated = 0.0f32;

    for (i, block) in blocks.iter().enumerate() {
        let block_h = estimated_block_height_for_width(block, page_width, font_size, font_system);
        let needed = if i == page_start {
            block_h
        } else {
            SPACING + block_h
        };

        if i != page_start && accumulated + needed > page_height {
            // Close current page, start a new one with this block.
            pages.push(PageRange {
                start: page_start,
                end: i,
            });
            page_start = i;
            accumulated = block_h;
        } else {
            accumulated += needed;
        }
    }

    // Push the final page.
    if page_start < blocks.len() {
        pages.push(PageRange {
            start: page_start,
            end: blocks.len(),
        });
    }

    if pages.is_empty() {
        pages.push(PageRange { start: 0, end: 0 });
    }

    PaginationLayout {
        page_height,
        page_width,
        pages,
    }
}

impl EpubViewer {
    /// Re-paginate the active chapter if the viewport size has changed.
    fn maybe_repaginate(&mut self) {
        if self.view_mode != ViewMode::Paginated {
            return;
        }
        let (vw, vh) = self.viewport_size.get();
        if vw <= 0.0 || vh <= 0.0 {
            return;
        }

        let space_s = theme::active().cosmic().spacing.space_s as f32;
        let space_xxs = theme::active().cosmic().spacing.space_xxs as f32;
        let max_content_width = 800.0 * (self.content_width_pct / 100.0);
        // In dual-page mode, each page gets half the width minus the gap.
        let per_page_width = if self.should_dual_page(vw) {
            let available = vw - space_xxs; // gap between pages
            (available / 2.0 - space_s * 2.0).min(max_content_width)
        } else {
            max_content_width.min(vw - space_s * 2.0)
        };
        let page_width = per_page_width;
        // Reserve space for the page indicator line at the bottom.
        // Apply the user-configurable height fraction to compensate for
        // inaccurate height estimates — a value < 1.0 leaves extra headroom.
        let page_height = (vh - space_s * 2.0 - 24.0) * self.page_height_fraction;

        let needs_recompute = match self.pagination_cache.get(&self.active_chapter) {
            Some(cached) => {
                (cached.page_height - page_height).abs() > 1.0
                    || (cached.page_width - page_width).abs() > 1.0
            }
            None => true,
        };

        if needs_recompute && let Some(chapter) = self.chapters.get(self.active_chapter) {
            let layout = paginate_blocks(
                &chapter.blocks,
                page_height,
                page_width,
                self.base_font_size,
                &mut self.font_system.borrow_mut(),
            );
            if self.current_page >= layout.pages.len() {
                self.current_page = layout.pages.len().saturating_sub(1);
            }
            self.pagination_cache.insert(self.active_chapter, layout);
        }

        // Consume deferred block index once a valid layout exists.
        if let Some(block_idx) = self.pending_block_index.take() {
            if let Some(layout) = self.pagination_cache.get(&self.active_chapter) {
                self.current_page = layout
                    .pages
                    .iter()
                    .position(|p| p.start <= block_idx && block_idx < p.end)
                    .unwrap_or(0);
            } else {
                // Layout still not available — put the index back.
                self.pending_block_index = Some(block_idx);
            }
        }
    }

    /// Total number of pages for the active chapter, or 0 if not paginated.
    fn total_pages(&self) -> usize {
        self.pagination_cache
            .get(&self.active_chapter)
            .map(|l| l.pages.len())
            .unwrap_or(0)
    }

    /// Convert current scroll_y position to approximate page index.
    fn scroll_y_to_page(&self) -> usize {
        if let Some(layout) = self.pagination_cache.get(&self.active_chapter)
            && let Some(chapter) = self.chapters.get(self.active_chapter)
        {
            let mut fs = self.font_system.borrow_mut();
            let mut y = 0.0f32;
            for (i, block) in chapter.blocks.iter().enumerate() {
                let h = estimated_block_height_for_width(
                    block,
                    layout.page_width,
                    self.base_font_size,
                    &mut fs,
                );
                if y + h > self.scroll_y {
                    return layout
                        .pages
                        .iter()
                        .position(|p| p.start <= i && i < p.end)
                        .unwrap_or(0);
                }
                y += h + 8.0;
            }
        }
        0
    }

    /// Convert current page index to approximate scroll_y position.
    fn page_to_scroll_y(&self) -> f32 {
        if let Some(layout) = self.pagination_cache.get(&self.active_chapter)
            && let Some(chapter) = self.chapters.get(self.active_chapter)
            && let Some(page) = layout.pages.get(self.current_page)
        {
            let mut fs = self.font_system.borrow_mut();
            let mut y = 0.0f32;
            for block in &chapter.blocks[..page.start] {
                y += estimated_block_height_for_width(
                    block,
                    layout.page_width,
                    self.base_font_size,
                    &mut fs,
                ) + 8.0;
            }
            return y;
        }
        0.0
    }

    /// Whether dual-page display should be active at the given viewport width.
    fn should_dual_page(&self, viewport_width: f32) -> bool {
        match self.dual_page {
            DualPageMode::On => true,
            DualPageMode::Off => false,
            DualPageMode::Auto => viewport_width > 1200.0,
        }
    }

    /// Find the block index closest to a given scroll_y offset using the
    /// default 80-char estimation (no width info available).
    fn approximate_block_at_scroll_y(&self, target_y: f32) -> Option<usize> {
        let chapter = self.chapters.get(self.active_chapter)?;
        let mut y = 0.0f32;
        for (i, block) in chapter.blocks.iter().enumerate() {
            let h = estimated_block_height(block);
            if y + h > target_y {
                return Some(i);
            }
            y += h + 8.0;
        }
        Some(chapter.blocks.len().saturating_sub(1))
    }

    /// How many pages to advance per navigation step (2 in dual-page mode, 1 otherwise).
    fn page_step(&self) -> usize {
        let (vw, _) = self.viewport_size.get();
        if self.should_dual_page(vw) { 2 } else { 1 }
    }

    /// Set `highlighted_block` to the current search match and navigate to it.
    fn navigate_to_search_match(&mut self) -> Task<Action<EpubViewerMessage>> {
        let Some(&block_idx) = self.search_matches.get(self.search_current) else {
            self.highlighted_block = None;
            return Task::none();
        };
        self.highlighted_block = Some(block_idx);

        match self.view_mode {
            ViewMode::Scroll => {
                // Estimate the y-position of the target block.
                let content_w = 800.0 * (self.content_width_pct / 100.0);
                let mut fs = self.font_system.borrow_mut();
                let y = self
                    .chapters
                    .get(self.active_chapter)
                    .map(|ch| {
                        ch.blocks[..block_idx].iter().fold(0.0f32, |acc, b| {
                            acc + estimated_block_height_for_width(
                                b,
                                content_w,
                                self.base_font_size,
                                &mut fs,
                            ) + 8.0
                        })
                    })
                    .unwrap_or(0.0);
                drop(fs);
                self.scroll_y = y;
                scrollable::scroll_to(
                    self.content_scroll_id.clone(),
                    scrollable::AbsoluteOffset { x: 0.0, y },
                )
            }
            ViewMode::Paginated => {
                if let Some(layout) = self.pagination_cache.get(&self.active_chapter) {
                    if let Some(page_idx) = layout
                        .pages
                        .iter()
                        .position(|p| p.start <= block_idx && block_idx < p.end)
                    {
                        self.current_page = page_idx;
                    }
                } else {
                    self.pending_block_index = Some(block_idx);
                }
                Task::none()
            }
        }
    }
}

fn styled_span<'a>(
    text_span: &'a TextSpan,
    family: font::Family,
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
        family,
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
    if let Some([r, g, b]) = text_span.color {
        s = s.color(cosmic::iced::Color::from_rgb8(r, g, b));
    }
    if let Some(em) = text_span.font_size_em {
        s = s.size(em * 16.0);
    }
    if let Some(href) = &text_span.link {
        s = s.link(EpubViewerMessage::FollowLink(href.clone()));
        // Only apply accent color if no explicit color was set
        if text_span.color.is_none() {
            s = s.color(theme::active().cosmic().accent_color());
        }
    }
    s
}

fn render_spans(
    spans: &[TextSpan],
    size: f32,
    family: font::Family,
) -> Element<'_, EpubViewerMessage> {
    let iced_spans: Vec<_> = spans.iter().map(|s| styled_span(s, family)).collect();
    rich_text(iced_spans).size(size).width(Length::Fill).into()
}

fn render_list_item_spans<'a>(
    prefix: String,
    spans: &'a [TextSpan],
    size: f32,
    family: font::Family,
) -> Element<'a, EpubViewerMessage> {
    let mut iced_spans: Vec<cosmic::iced::widget::text::Span<'a, EpubViewerMessage>> =
        Vec::with_capacity(spans.len() + 1);
    iced_spans.push(span(prefix));
    iced_spans.extend(spans.iter().map(|s| styled_span(s, family)));
    rich_text(iced_spans).size(size).width(Length::Fill).into()
}

/// Map `BlockStyle.text_align` to an iced `Horizontal` alignment.
fn text_align_horizontal(style: &BlockStyle) -> cosmic::iced::alignment::Horizontal {
    match style.text_align {
        Some(TextAlign::Center) => cosmic::iced::alignment::Horizontal::Center,
        Some(TextAlign::Right) => cosmic::iced::alignment::Horizontal::Right,
        _ => cosmic::iced::alignment::Horizontal::Left,
    }
}

/// Wrap `el` in a container that applies margin-top / margin-bottom from `style`.
/// If no margins are set the element is returned as-is.
fn apply_text_align<'a>(
    el: Element<'a, EpubViewerMessage>,
    style: &BlockStyle,
) -> Element<'a, EpubViewerMessage> {
    use cosmic::iced::widget::container;
    let has_margin = style.margin_top_em.is_some() || style.margin_bottom_em.is_some();
    if !has_margin {
        return el;
    }
    // Convert em to pixels using 16px base
    let top = style.margin_top_em.unwrap_or(0.0) * 16.0;
    let bottom = style.margin_bottom_em.unwrap_or(0.0) * 16.0;
    container(el)
        .padding(cosmic::iced::Padding {
            top,
            bottom,
            left: 0.0,
            right: 0.0,
        })
        .width(Length::Fill)
        .into()
}

fn render_block<'a>(
    block: &'a ContentBlock,
    highlight: BlockHighlight,
    font_size: f32,
    family: font::Family,
    document: &'a Document,
    chapter_href: &'a str,
    epub_document: Option<&'a EpubDocument>,
) -> Element<'a, EpubViewerMessage> {
    let inner = render_block_inner(
        block,
        font_size,
        family,
        document,
        chapter_href,
        epub_document,
    );
    match highlight {
        BlockHighlight::None => inner,
        BlockHighlight::Current => widget::container(inner)
            .style(|theme: &cosmic::Theme| widget::container::Style {
                background: Some(highlight_background(theme).into()),
                text_color: Some(highlight_text_color(theme)),
                border: Border {
                    radius: theme.cosmic().corner_radii.radius_xl.into(),
                    width: 0.0,
                    color: Color::TRANSPARENT,
                },
                ..Default::default()
            })
            .width(Length::Fill)
            .into(),
        BlockHighlight::SearchMatch => widget::container(inner)
            .style(|theme: &cosmic::Theme| widget::container::Style {
                background: Some(search_match_background(theme).into()),
                border: Border {
                    radius: theme.cosmic().corner_radii.radius_s.into(),
                    width: 0.0,
                    color: Color::TRANSPARENT,
                },
                ..Default::default()
            })
            .width(Length::Fill)
            .into(),
    }
}

fn render_block_inner<'a>(
    block: &'a ContentBlock,
    font_size: f32,
    family: font::Family,
    document: &'a Document,
    chapter_href: &'a str,
    epub_document: Option<&'a EpubDocument>,
) -> Element<'a, EpubViewerMessage> {
    let cosmic_theme::Spacing {
        space_xxs, space_s, ..
    } = theme::active().cosmic().spacing;

    let font = Font {
        family,
        ..Font::default()
    };

    match block {
        ContentBlock::Heading {
            level,
            spans,
            text,
            style,
        } => {
            let base_size = match level {
                1 => font_size * 2.0,
                2 => font_size * 1.75,
                3 => font_size * 1.5,
                4 => font_size * 1.25,
                _ => font_size * 1.125,
            };
            let size = style
                .font_size_em
                .map(|em| em * base_size)
                .unwrap_or(base_size);
            let align = text_align_horizontal(style);
            if spans.is_empty() {
                return apply_text_align(
                    match level {
                        1 => widget::text::title1(text)
                            .font(font)
                            .width(Length::Fill)
                            .align_x(align)
                            .into(),
                        2 => widget::text::title2(text)
                            .font(font)
                            .width(Length::Fill)
                            .align_x(align)
                            .into(),
                        3 => widget::text::title3(text)
                            .font(font)
                            .width(Length::Fill)
                            .align_x(align)
                            .into(),
                        4 => widget::text::title4(text)
                            .font(font)
                            .width(Length::Fill)
                            .align_x(align)
                            .into(),
                        _ => widget::text::heading(text)
                            .font(font)
                            .width(Length::Fill)
                            .align_x(align)
                            .into(),
                    },
                    style,
                );
            }
            apply_text_align(render_spans(spans, size, family), style)
        }
        ContentBlock::Paragraph { spans, text, style } => {
            let size = style
                .font_size_em
                .map(|em| em * font_size)
                .unwrap_or(font_size);
            let align = text_align_horizontal(style);
            if spans.is_empty() {
                return apply_text_align(
                    widget::text::body(text)
                        .size(font_size)
                        .font(font)
                        .width(Length::Fill)
                        .align_x(align)
                        .into(),
                    style,
                );
            }
            apply_text_align(render_spans(spans, size, family), style)
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
                col = col.push(render_block_inner(
                    child,
                    font_size,
                    family,
                    document,
                    chapter_href,
                    epub_document,
                ));
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
                        widget::text::body(format!("  \u{2022} {}", item.text))
                            .size(font_size)
                            .font(font)
                            .width(Length::Fill),
                    );
                } else {
                    col = col.push(render_list_item_spans(
                        "  \u{2022} ".to_string(),
                        &item.spans,
                        font_size,
                        family,
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
                        widget::text::body(format!("  {n}. {}", item.text))
                            .size(font_size)
                            .font(font)
                            .width(Length::Fill),
                    );
                } else {
                    let prefix = format!("  {n}. ");
                    col = col.push(render_list_item_spans(
                        prefix,
                        &item.spans,
                        font_size,
                        family,
                    ));
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
                    .font(font)
                    .width(Length::Fill)
                    .into()
            } else {
                widget::Space::new(Length::Fill, 0).into()
            }
        }
        ContentBlock::Svg { content, .. } => {
            // Process SVG content to resolve embedded image references
            let processed_content = if let Some(epub_doc) = epub_document {
                epub::content::resolve_svg_images(content, chapter_href, &mut |img_path| {
                    match epub_doc.resolve_resource(img_path) {
                        Ok(img_data) => {
                            let media_type = epub::content::guess_media_type(img_path);
                            Some((img_data, media_type))
                        }
                        Err(_) => None,
                    }
                })
            } else {
                // Fallback to original content if no epub document is available
                content.clone()
            };

            let handle = widget::svg::Handle::from_memory(processed_content.into_bytes());
            widget::svg(handle)
                .width(Length::Shrink)
                .content_fit(cosmic::iced::ContentFit::ScaleDown)
                .apply(widget::container)
                .width(Length::Fill)
                .align_x(Horizontal::Center)
                .into()
        }
        ContentBlock::Table { rows } => render_table(rows, font_size, family),
        ContentBlock::HorizontalRule => widget::divider::horizontal::default().into(),
        ContentBlock::Footnote { blocks, .. } => render_footnote(
            blocks,
            font_size,
            family,
            document,
            chapter_href,
            epub_document,
        ),
        ContentBlock::Figure {
            blocks,
            caption,
            caption_text,
        } => {
            let caption_size = (font_size * 0.85).max(10.0);
            let mut col = widget::column::with_capacity(blocks.len() + 1)
                .spacing(space_xxs)
                .width(Length::Fill);
            for block in blocks {
                col = col.push(render_block_inner(
                    block,
                    font_size,
                    family,
                    document,
                    chapter_href,
                    epub_document,
                ));
            }
            if !caption.is_empty() {
                col = col.push(render_spans(caption, caption_size, family));
            } else if !caption_text.is_empty() {
                let caption_el: Element<'_, EpubViewerMessage> =
                    widget::text::caption(caption_text.as_str())
                        .size(caption_size)
                        .font(Font {
                            family,
                            style: font::Style::Italic,
                            ..Font::default()
                        })
                        .width(Length::Fill)
                        .align_x(Horizontal::Center)
                        .into();
                col = col.push(caption_el);
            }
            col.into()
        }
        ContentBlock::Anchor { .. } => {
            widget::Space::new(Length::Fixed(0.0), Length::Fixed(0.0)).into()
        }
    }
}

fn render_footnote<'a>(
    blocks: &'a [ContentBlock],
    font_size: f32,
    family: font::Family,
    document: &'a Document,
    chapter_href: &'a str,
    epub_document: Option<&'a EpubDocument>,
) -> Element<'a, EpubViewerMessage> {
    let cosmic_theme::Spacing {
        space_xxs, space_s, ..
    } = theme::active().cosmic().spacing;

    let caption_size = (font_size * 0.8).max(10.0);

    let mut col = widget::column::with_capacity(blocks.len())
        .spacing(space_xxs)
        .width(Length::Fill);

    for block in blocks {
        let el: Element<_> = match block {
            ContentBlock::Paragraph { spans, text, .. } => {
                if spans.is_empty() {
                    widget::text::caption(text)
                        .size(caption_size)
                        .width(Length::Fill)
                        .into()
                } else {
                    render_spans(spans, caption_size, family)
                }
            }
            _ => render_block_inner(
                block,
                font_size,
                family,
                document,
                chapter_href,
                epub_document,
            ),
        };
        col = col.push(el);
    }

    widget::container(col)
        .padding([space_xxs, space_s])
        .class(Container::Secondary)
        .width(Length::Fill)
        .into()
}

fn render_table(
    rows: &[Vec<TableCell>],
    font_size: f32,
    family: font::Family,
) -> Element<'_, EpubViewerMessage> {
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
            col = col.push(widget::divider::horizontal::heavy());
            divider_inserted = true;
        } else {
            col = col.push(widget::divider::horizontal::light());
        }
        if is_header_row {
            seen_header = true;
        }

        let mut row_widget = widget::row().width(Length::Fill);

        for cell in row {
            let cell_spans: Vec<cosmic::iced::widget::text::Span<'_, EpubViewerMessage>> =
                if !cell.spans.is_empty() {
                    cell.spans
                        .iter()
                        .map(|s| {
                            let mut sp = styled_span(s, family);
                            // Force bold for header cells whose spans aren't already bold.
                            if cell.is_header && !s.style.bold {
                                let font = sp.font.unwrap_or_default();
                                sp = sp.font(Font {
                                    weight: font::Weight::Bold,
                                    ..font
                                });
                            }
                            sp
                        })
                        .collect()
                } else if !cell.text.is_empty() {
                    let mut s = span(cell.text.as_str());
                    if cell.is_header {
                        s = s.font(Font {
                            family,
                            weight: font::Weight::Bold,
                            ..Font::default()
                        });
                    } else {
                        s = s.font(Font {
                            family,
                            ..Font::default()
                        });
                    }
                    vec![s]
                } else {
                    vec![]
                };

            let cell_content: Element<'_, EpubViewerMessage> = if !cell_spans.is_empty() {
                rich_text(cell_spans)
                    .size(font_size)
                    .width(Length::Fill)
                    .into()
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
