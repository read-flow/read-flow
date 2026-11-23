pub(crate) mod measure;
#[cfg(test)]
mod pagination_tests;
mod render;
#[cfg(test)]
pub(crate) mod test_helper;

use std::cell::Cell;
use std::cell::RefCell;
use std::collections::HashMap;
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
use cosmic::iced::Length;
use cosmic::iced::advanced::graphics::text::Paragraph;
use cosmic::iced::advanced::text::Paragraph as _;
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::alignment::Vertical;
use cosmic::iced::core::SmolStr;
use cosmic::iced::font;
use cosmic::iced::keyboard::Key;
use cosmic::iced::keyboard::Modifiers;
use cosmic::iced::keyboard::key::Named;
use cosmic::iced::widget::scrollable;
use cosmic::iced::widget::text_editor;
use cosmic::task;
use cosmic::theme;
use cosmic::theme::Container;
use cosmic::widget;
use epub::BlockStyle;
use epub::ContentBlock;
use epub::Document as EpubDocumentTrait;
use epub::EpubDocument;
use epub::ListItem;
use epub::NavEntry;
use epub::StyleSheet;
use epub::TextSpan;
use read_flow_core::Builder;
use read_flow_widgets::FontPicker;

use crate::ICON_SIZE;
use crate::aggregator::Document;
use crate::app::ContextView;
use crate::app::ReadFlow;
use crate::client::ClientSelector;
use crate::document_provider::DocumentProvider;
use crate::fl;
use crate::fonts::fonts;
use crate::layout::full_page;
use crate::page::Page;
use crate::page::epub_viewer::measure::BlockMeasurer;
use crate::page::epub_viewer::measure::block_height;
use crate::page::epub_viewer::render::RenderContext;
use crate::page::epub_viewer::render::render_partial_list;
use crate::page::epub_viewer::render::render_partial_paragraph;
use crate::page::epub_viewer::render::render_partial_preformatted;
use crate::page::image_viewer::ViewerImage;
use crate::reading_progress::ReadingProgress;
use crate::reading_progress::Viewer;
use crate::reading_progress::ViewerPosition;
use crate::reading_progress::extract;

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

// --- Persistent reader preferences ---

/// Config version for EPUB reader preferences (individual key access).
const EPUB_PREFS_VERSION: u64 = 1;

/// Config key for the saved font family.
const KEY_FONT_FAMILY: &str = "epub_font_family";

/// Config key for the saved base font size (stored as integer pixels).
const KEY_BASE_FONT_SIZE: &str = "epub_base_font_size";

/// Config key for the saved view mode ("scroll" or "paginated").
const KEY_VIEW_MODE: &str = "epub_view_mode";

/// Config key for dual-page mode ("auto", "off", "on").
const KEY_DUAL_PAGE: &str = "epub_dual_page";

/// Config key for the page margin in pixels (stored as u32, 0–128).
const KEY_PAGE_MARGIN: &str = "epub_page_margin";

/// All EPUB reader preferences stored in cosmic-config.
struct EpubPrefs {
    font_family: FontFamily,
    base_font_size: f32,
    view_mode: ViewMode,
    dual_page: DualPageMode,
    page_margin: f32,
}

impl Default for EpubPrefs {
    fn default() -> Self {
        EpubPrefs {
            font_family: FontFamily::default(),
            base_font_size: 16.0,
            view_mode: ViewMode::default(),
            dual_page: DualPageMode::default(),
            page_margin: 16.0,
        }
    }
}

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

/// Load all saved EPUB reader preferences from cosmic-config.
fn load_epub_prefs() -> EpubPrefs {
    let Ok(ctx) = cosmic_config::Config::new(ReadFlow::APP_ID, EPUB_PREFS_VERSION) else {
        return EpubPrefs::default();
    };
    let mut prefs = EpubPrefs::default();

    let family_str: String = ctx.get(KEY_FONT_FAMILY).unwrap_or_default();
    prefs.font_family = str_to_font_family(&family_str);

    let size_px: u32 = ctx.get(KEY_BASE_FONT_SIZE).unwrap_or(16);
    prefs.base_font_size = (size_px as f32).clamp(12.0, 24.0);

    if let Ok(mode_str) = ctx.get::<String>(KEY_VIEW_MODE) {
        prefs.view_mode = match mode_str.as_str() {
            "scroll" => ViewMode::Scroll,
            "paginated" => ViewMode::Paginated,
            _ => ViewMode::default(),
        };
    }

    if let Ok(dp_str) = ctx.get::<String>(KEY_DUAL_PAGE) {
        prefs.dual_page = match dp_str.as_str() {
            "off" => DualPageMode::Off,
            "on" => DualPageMode::On,
            _ => DualPageMode::Auto,
        };
    }

    if let Ok(margin) = ctx.get::<u32>(KEY_PAGE_MARGIN) {
        prefs.page_margin = (margin as f32).clamp(0.0, 128.0);
    }

    prefs
}

/// Save all EPUB reader preferences to cosmic-config. Unit tests change
/// preferences without saving them, leaving the user's configuration alone.
fn save_epub_prefs(prefs: &EpubPrefs) {
    if cfg!(test) {
        return;
    }
    let Ok(ctx) = cosmic_config::Config::new(ReadFlow::APP_ID, EPUB_PREFS_VERSION) else {
        return;
    };
    let _ = ctx.set(KEY_FONT_FAMILY, font_family_to_str(prefs.font_family));
    let _ = ctx.set(KEY_BASE_FONT_SIZE, prefs.base_font_size.round() as u32);
    let mode_str = match prefs.view_mode {
        ViewMode::Scroll => "scroll",
        ViewMode::Paginated => "paginated",
    };
    let _ = ctx.set(KEY_VIEW_MODE, mode_str);
    let dp_str = match prefs.dual_page {
        DualPageMode::Auto => "auto",
        DualPageMode::Off => "off",
        DualPageMode::On => "on",
    };
    let _ = ctx.set(KEY_DUAL_PAGE, dp_str);
    let _ = ctx.set(KEY_PAGE_MARGIN, prefs.page_margin.round() as u32);
}

// --- View mode and pagination types ---

/// Which reading mode the viewer uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ViewMode {
    Scroll,
    #[default]
    Paginated,
}

/// A position inside a block where a page starts or ends: before character
/// `char_offset` of the block's child `child`, `char_offset` 0 being before
/// the child. What the child is depends on the block:
///
/// - paragraph or code block: its text, child 0;
/// - list: item `child`;
/// - block quote, footnote or figure: child block `child`, and `char_offset`
///   is a cut in that block as one number (see [`container_child_cut`]): a
///   character of a paragraph or code block, a character counted over the
///   items of a list, each item one more than its length, or the child
///   before which a nested container is cut, which is only cut between its
///   children. Child `len` of a figure is its caption;
/// - table: row `child`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
struct BlockCut {
    child: usize,
    char_offset: usize,
}

/// A page is a contiguous range of blocks from the chapter's block list.
/// Stored as \[start .. end) (exclusive end).
#[derive(Clone, Debug)]
struct PageRange {
    start: usize,
    /// Where in `blocks[start]` the page starts (the default cut: at its
    /// beginning).
    start_cut: BlockCut,
    end: usize,
    /// Where in `blocks[end - 1]` the page ends (`None`: at its end).
    end_cut: Option<BlockCut>,
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

// --- Core types ---

#[derive(Clone, Debug)]
pub(crate) struct EpubChapter {
    label: String,
    /// Resolved zip path for this spine item (e.g. `OEBPS/Text/ch1.xhtml`).
    href: String,
    blocks: Vec<ContentBlock>,
    /// DOM node paths parallel to `blocks`, one per top-level block.
    /// Each path is a sequence of 0-based element-child indices starting from
    /// within `<html>` (body index first, then block index within body).
    block_paths: Vec<Vec<u32>>,
    /// Stable image handles keyed by the raw pointer of each image's data `Vec`.
    /// Created once so the handle ID is stable across frames; required because
    /// the wgpu renderer decodes images asynchronously keyed by handle ID —
    /// a fresh `Handle::from_bytes` call per frame would produce a new ID each
    /// time, preventing the decoded texture from ever being found in the cache.
    /// The pointer is valid for the lifetime of this chapter (data lives in `blocks`).
    image_handles: HashMap<usize, widget::image::Handle>,
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

#[derive(Debug, Clone)]
pub enum EpubViewerOutput {
    /// Carries the fingerprint and the reading progress to persist, if any.
    Close(Fingerprint, Option<ReadingProgress>),
    /// Open the image viewer page for the given image.
    OpenImageViewer(ViewerImage),
    /// Request the App to navigate to (activate) this viewer's page.
    Activate,
    OpenDocumentDetails(Box<Document>),
    /// Open the current document in the MuPDF viewer, as a separate page.
    OpenInMuPdfViewer(Box<Document>),
    /// Open the current document in the system's default application.
    OpenInExternalViewer(Box<Document>),
}

#[derive(Debug, Clone)]
/// @feature: reading.progress
pub enum EpubViewerMessage {
    EpubLoaded(String, Vec<EpubChapter>, Option<CloneableEpubDocument>),
    /// Carries restored reading position from saved progress.
    ReadingProgressLoaded(ReadingPosition),
    SelectChapter(usize),
    SelectNavEntry(usize),
    ShowRawHtml(bool),
    RawHtmlAction(text_editor::Action),
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
    /// Set the page margin in pixels (0–128) for paginated mode.
    SetPageMargin(f32),
    /// Set the font family for rendering (index into FontFamily::ALL).
    SetFontFamily(FontFamily),
    /// Set the base body font size in pixels (12–24).
    SetBaseFontSize(f32),
    /// Toggle the top-level EPUB node's collapsed state in the nav sidebar.
    ToggleTopLevelNav,
    /// Toggle a specific nav-entry's collapsed state (by its index in `nav_entries`).
    ToggleNavEntry(usize),
    /// Expand all nav-sidebar entries (clear all collapsed state).
    ExpandAllNavEntries,
    /// Collapse all nav-sidebar entries that have children.
    CollapseAllNavEntries,
    /// The EPUB file could not be opened; carries the error message.
    LoadFailed(String),
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
    /// Copy a code block's plain text to the clipboard.
    CopyCodeBlock(String),
    /// Open the image viewer page for the given image.
    OpenImageViewer(ViewerImage),
    /// Installed font families finished loading (slow, done off-thread).
    FontOptionsLoaded(Vec<&'static str>),
    /// Font picker opened (dropdown now visible).
    FontPickerOpen,
    /// Font picker closed without selection.
    FontPickerClose,
    /// User typed in the font picker filter input.
    FontPickerQuery(String),
    /// The nav-sidebar close button was clicked; compute the current
    /// progress snapshot now (at click time) rather than on every redraw.
    RequestClose,
    Out(EpubViewerOutput),
}

// --- EpubViewer page ---

/// @feature: reading.epub_viewer
pub struct EpubViewer {
    fingerprint: Fingerprint,
    document: Document,
    epub_document: Option<CloneableEpubDocument>,
    file_path: Option<PathBuf>,
    load_error: Option<String>,
    title: String,
    chapters: Vec<EpubChapter>,
    active_chapter: usize,
    initial_chapter: Option<usize>,
    /// Scroll position (absolute y offset in pixels) within the current chapter.
    scroll_y: f32,
    modifiers: Modifiers,
    show_raw_html: bool,
    raw_html_content: text_editor::Content,
    content_scroll_id: widget::Id,
    /// Current reading mode: scroll (default) or paginated.
    view_mode: ViewMode,
    /// In paginated mode, the current page index within the active chapter.
    current_page: usize,
    /// Cached pagination layout per chapter index.
    pagination_cache: HashMap<usize, PaginationLayout>,
    /// Cached per-block heights for each chapter, keyed by chapter index.
    /// Value is `(content_width, font_size, heights_vec)`.  Invalidated when
    /// `base_font_size` or the EPUB document changes.
    block_heights_cache: HashMap<usize, (f32, f32, Vec<f32>)>,
    /// Lays out blocks to measure their heights (via RefCell, since `view()`
    /// paginates with `&self`).
    measurer: RefCell<BlockMeasurer>,
    /// Most recently observed viewport dimensions, set from the `responsive`
    /// closure (via Cell, since `view()` takes `&self`).
    viewport_size: Cell<(f32, f32)>,
    /// Size of the area the pages are laid out in (paginated mode), recorded
    /// during view() so pagination measures against the space the pages get.
    page_area: Cell<(f32, f32)>,
    /// Whether to render two pages side by side in paginated mode.
    dual_page: DualPageMode,
    /// Deferred block index for page restoration.  Set when reading progress
    /// is loaded but pagination hasn't run yet (viewport_size still 0×0).
    /// Consumed by `maybe_repaginate()` once a valid layout is available.
    pending_block_index: Option<usize>,
    /// CFI node path stored when progress is loaded before chapters are ready.
    /// Resolved to a block index in `EpubLoaded` once chapters are available.
    pending_node_path: Vec<u32>,
    /// Font family used for rendering EPUB content.
    font_family: FontFamily,
    /// All font family options (labels) for the picker dropdown.
    font_picker_options: Vec<&'static str>,
    /// Current filter query typed into the font picker input.
    font_picker_query: String,
    /// Whether the font picker dropdown is open.
    font_picker_focused: bool,
    /// Ordered nav entries from the EPUB TOC (with depth and fragment-preserving hrefs).
    nav_entries: Vec<NavEntry>,
    /// Index of the most recently activated nav entry, for precise sidebar highlighting.
    active_nav_entry: Option<usize>,
    /// Whether the top-level EPUB node in the nav sidebar is collapsed.
    top_level_nav_collapsed: bool,
    /// Indices of nav entries whose sub-entries are collapsed in the nav sidebar.
    collapsed_nav_entries: std::collections::HashSet<usize>,
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
    /// Page margin in pixels applied to all four sides of the page content area (0–128).
    page_margin: f32,
    /// Whether developer-only controls (e.g. raw HTML view) are shown, set via `--debug`.
    debug: bool,
}

impl EpubViewer {
    pub fn new(
        document: Document,
        document_provider: Arc<DocumentProvider>,
        debug: bool,
    ) -> (Self, Task<Action<EpubViewerMessage>>) {
        let fingerprint = document
            .contents
            .first()
            .map(|c| c.fingerprint.clone())
            .unwrap_or_default();

        let sources = document.sources_by_priority();
        let local_source = sources
            .iter()
            .find(|(_, s)| s.client == ClientSelector::Local);
        // Archive members are extracted to a cached temp file first.
        let file_path = local_source.and_then(|(_, s)| match s.local_read_path() {
            Ok(path) => Some(path),
            Err(e) => {
                tracing::error!("cannot resolve local read path for {}: {e}", s.path);
                None
            }
        });

        let saved_prefs = load_epub_prefs();

        let viewer = EpubViewer {
            fingerprint: fingerprint.clone(),
            document,
            epub_document: None, // Will be loaded when chapters are loaded
            file_path: file_path.clone(),
            load_error: None,
            title: String::new(),
            chapters: Vec::new(),
            active_chapter: 0,
            initial_chapter: None,
            scroll_y: 0.0,
            modifiers: Modifiers::default(),
            show_raw_html: false,
            raw_html_content: text_editor::Content::new(),
            content_scroll_id: widget::Id::unique(),
            view_mode: saved_prefs.view_mode,
            current_page: 0,
            pagination_cache: HashMap::new(),
            block_heights_cache: HashMap::new(),
            measurer: RefCell::default(),
            viewport_size: Cell::new((0.0, 0.0)),
            page_area: Cell::new((0.0, 0.0)),
            dual_page: saved_prefs.dual_page,
            pending_block_index: None,
            pending_node_path: Vec::new(),
            font_family: saved_prefs.font_family,
            font_picker_options: Vec::new(),
            font_picker_query: String::new(),
            font_picker_focused: false,
            nav_entries: Vec::new(),
            active_nav_entry: None,
            top_level_nav_collapsed: false,
            collapsed_nav_entries: std::collections::HashSet::new(),
            base_font_size: saved_prefs.base_font_size,
            highlighted_block: None,
            search_visible: false,
            search_query: String::new(),
            search_matches: Vec::new(),
            search_current: 0,
            search_input_id: widget::Id::unique(),
            page_margin: saved_prefs.page_margin,
            debug,
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
                |result| match result {
                    Ok((title, chapters, epub_doc)) => {
                        cosmic::action::app(EpubViewerMessage::EpubLoaded(
                            title,
                            chapters,
                            Some(CloneableEpubDocument::from(epub_doc)),
                        ))
                    }
                    Err(e) => {
                        tracing::error!("failed to open EPUB: {e}");
                        cosmic::action::app(EpubViewerMessage::LoadFailed(e.to_string()))
                    }
                },
            ));
        }

        // Fetch reading progress
        let fp = fingerprint;
        tasks.push(Task::perform(
            async move {
                let aggregator = document_provider.aggregator.read().await;
                match aggregator.get_reading_state(&fp).await {
                    Ok(Some(state)) => extract(&state.position, Viewer::Epub)
                        .map(resolve_reading_position)
                        .unwrap_or_default(),
                    Ok(None) => ReadingPosition::default(),
                    Err(e) => {
                        tracing::warn!("failed to load reading state: {e}");
                        ReadingPosition::default()
                    }
                }
            },
            |pos| cosmic::action::app(EpubViewerMessage::ReadingProgressLoaded(pos)),
        ));

        // Font enumeration shells out and is slow on first use — load the
        // picker options asynchronously so opening a book doesn't block.
        tasks.push(Task::perform(
            async {
                tokio::task::spawn_blocking(|| {
                    FontFamily::all().into_iter().map(|f| f.label()).collect()
                })
                .await
                .unwrap_or_default()
            },
            |options| cosmic::action::app(EpubViewerMessage::FontOptionsLoaded(options)),
        ));

        (viewer, Task::batch(tasks))
    }

    /// Snapshot the current viewer preferences and persist them to cosmic-config.
    fn save_current_prefs(&self) {
        save_epub_prefs(&EpubPrefs {
            font_family: self.font_family,
            base_font_size: self.base_font_size,
            view_mode: self.view_mode,
            dual_page: self.dual_page,
            page_margin: self.page_margin,
        });
    }

    pub fn display_name(&self) -> String {
        if !self.title.is_empty() {
            return self.title.clone();
        }
        let path = self
            .document
            .contents
            .first()
            .and_then(|c| c.sources.first())
            .map(|s| s.path.as_str())
            .unwrap_or("EPUB");
        Path::new(path)
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("EPUB")
            .to_string()
    }

    fn view_search_bar(&self) -> Element<'_, EpubViewerMessage> {
        let cosmic_theme::Spacing {
            space_xxs, space_s, ..
        } = theme::active().cosmic().spacing;

        let input = widget::search_input(fl!("epub-viewer-search-placeholder"), &self.search_query)
            .id(self.search_input_id.clone())
            .on_input(EpubViewerMessage::SetSearchQuery)
            .on_submit(|_| EpubViewerMessage::SearchNext)
            .on_clear(EpubViewerMessage::CloseSearch)
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
            widget::Space::new().width(Length::Shrink).into()
        };

        widget::container(
            widget::Row::new()
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
                column = column.push(
                    widget::text_editor::text_editor(&self.raw_html_content)
                        .on_action(EpubViewerMessage::RawHtmlAction)
                        .font(cosmic::font::mono())
                        .apply(widget::container)
                        .width(Length::Fill),
                );
            } else {
                let ctx = self.scroll_render_context(chapter);
                for (idx, block) in chapter.blocks.iter().enumerate() {
                    let highlight = if self.highlighted_block == Some(idx) {
                        BlockHighlight::Current
                    } else if !self.search_matches.is_empty() && self.search_matches.contains(&idx)
                    {
                        BlockHighlight::SearchMatch
                    } else {
                        BlockHighlight::None
                    };
                    column = column.push(ctx.render_block(block, highlight));
                }
            }

            let paper = widget::container(column)
                .padding(space_s)
                .max_width(800.0)
                .width(Length::Fill)
                .style(move |theme: &cosmic::Theme| paper_background(theme));

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
            widget::Space::new()
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        }
    }

    fn view_content_paginated(&self) -> Element<'_, EpubViewerMessage> {
        let cosmic_theme::Spacing {
            space_s, space_xxs, ..
        } = theme::active().cosmic().spacing;

        let chapter = match self.chapters.get(self.active_chapter) {
            Some(ch) => ch,
            None => {
                return widget::Space::new()
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into();
            }
        };

        let current_page = self.current_page;
        let active_chapter = self.active_chapter;
        let show_raw_html = self.show_raw_html;

        let page_margin = self.page_margin;

        widget::responsive(move |size| {
            // Store viewport size so update() can trigger re-pagination.
            self.viewport_size.set((size.width, size.height));

            // In raw HTML mode, fall back to a simple scrollable view.
            if show_raw_html {
                return widget::container(
                    widget::text_editor::text_editor(&self.raw_html_content)
                        .on_action(EpubViewerMessage::RawHtmlAction)
                        .font(cosmic::font::mono())
                        .height(Length::Fill),
                )
                .style(|theme: &cosmic::Theme| desk_background(theme))
                .width(Length::Fill)
                .height(Length::Fill)
                .into();
            }

            let dual = self.should_dual_page(size.width);

            // The pages are laid out inside the exact area left for them, so
            // pagination measures against the space the text really gets.
            let pages = widget::responsive(move |area| {
                self.page_area.set((area.width, area.height));

                // Use cached layout if available, otherwise compute on-the-fly so
                // the first render after switching to paginated mode isn't empty.
                let computed_layout;
                let layout = match self.pagination_cache.get(&active_chapter) {
                    Some(l) => l,
                    None => {
                        let (cw, ch) = page_text_size(
                            (area.width, area.height),
                            dual,
                            space_xxs as f32,
                            page_margin,
                        );
                        computed_layout = paginate_blocks(
                            &chapter.blocks,
                            ch,
                            cw,
                            space_xxs as f32,
                            &self.page_render_context(chapter, ch),
                            &mut self.measurer.borrow_mut(),
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
                let make_paper = |page_idx: usize| -> Element<'_, EpubViewerMessage> {
                    let page_range = layout.pages.get(page_idx);

                    let mut column = widget::Column::new().spacing(space_xxs).width(Length::Fill);
                    if let Some(range) = page_range {
                        let block_count = range.end - range.start;
                        for (i, block) in chapter.blocks[range.start..range.end].iter().enumerate()
                        {
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
                            let from = if i == 0 {
                                range.start_cut
                            } else {
                                BlockCut::default()
                            };
                            let to = if i + 1 == block_count {
                                range.end_cut
                            } else {
                                None
                            };
                            column = column.push(render_block_part(
                                &self.page_render_context(chapter, layout.page_height),
                                block,
                                from,
                                to,
                                highlight,
                            ));
                        }
                    }
                    let page_indicator = widget::container(
                        widget::text::caption(format!("{} / {}", page_idx + 1, total))
                            .width(Length::Fill)
                            .align_x(Horizontal::Center),
                    )
                    .height(PAGE_INDICATOR_HEIGHT);

                    // Wrap blocks in a scrollable so content that overflows the
                    // page is still accessible. The inner container caps the text
                    // column at MAX_PAGE_WIDTH, and a centering wrapper keeps it
                    // horizontally centered within the paper.
                    let paper_content = widget::Column::new()
                        .push(
                            widget::scrollable(
                                widget::container(
                                    widget::container(column)
                                        .id(page_text_id())
                                        .padding(page_margin)
                                        .max_width(MAX_PAGE_WIDTH)
                                        .width(Length::Fill),
                                )
                                .width(Length::Fill)
                                .align_x(Horizontal::Center),
                            )
                            .width(Length::Fill)
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

                // Single or dual page.
                if dual {
                    let left_paper = make_paper(current_page);
                    let right_paper = if current_page + 1 < total {
                        make_paper(current_page + 1)
                    } else {
                        // Empty right page when on the last page.
                        widget::container(
                            widget::Space::new()
                                .width(Length::Fill)
                                .height(Length::Fill),
                        )
                        .style(move |theme: &cosmic::Theme| paper_background(theme))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .into()
                    };
                    widget::Row::new()
                        .push(left_paper)
                        .spacing(space_xxs)
                        .push(right_paper)
                        .height(Length::Fill)
                        .into()
                } else {
                    make_paper(current_page)
                }
            });

            // Cap the reading area so wide screens don't produce huge empty desk
            // margins. The click zones are FillPortion(1) each and the center is
            // FillPortion(10) when the viewport is wide enough. When the viewport
            // is narrower than max_row_width the page width is already constrained,
            // so the click zones shrink to their icon size to give the center the
            // maximum available space.
            let center_pages_width = if dual {
                MAX_PAGE_WIDTH * 2.0 + space_xxs as f32
            } else {
                MAX_PAGE_WIDTH
            };
            let max_row_width = center_pages_width / 0.8;
            let (zone_length, center_length) = if size.width < max_row_width {
                (
                    Length::Fixed((ICON_SIZE + space_s * 2) as f32),
                    Length::Fill,
                )
            } else {
                (Length::FillPortion(1), Length::FillPortion(10))
            };

            // Outer "desk" with click-to-turn zones.
            let left_zone = widget::mouse_area(
                widget::container(widget::icon::from_name("go-previous-symbolic").size(ICON_SIZE))
                    .width(zone_length)
                    .height(Length::Fill)
                    .align_x(Horizontal::Center)
                    .align_y(Vertical::Center),
            )
            .on_press(EpubViewerMessage::PreviousPage);

            let center = widget::container(pages)
                .width(center_length)
                .height(Length::Fill);

            let right_zone = widget::mouse_area(
                widget::container(widget::icon::from_name("go-next-symbolic").size(ICON_SIZE))
                    .width(zone_length)
                    .height(Length::Fill)
                    .align_x(Horizontal::Center)
                    .align_y(Vertical::Center),
            )
            .on_press(EpubViewerMessage::NextPage);

            let inner = widget::container(
                widget::Row::new()
                    .push(left_zone)
                    .push(center)
                    .push(right_zone)
                    .height(Length::Fill),
            )
            .max_width(max_row_width)
            .width(Length::Fill)
            .height(Length::Fill);

            widget::container(inner)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Horizontal::Center)
                .into()
        })
        .into()
    }
}

impl Page for EpubViewer {
    type Message = EpubViewerMessage;

    fn view(&self) -> Element<'_, EpubViewerMessage> {
        if self.file_path.is_none() {
            let no_source = widget::Column::new()
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
                    "epub-viewer-load-error",
                    error = error.as_str()
                )));

            return error_view.apply(full_page);
        }

        if self.chapters.is_empty() {
            let loading = widget::Column::new()
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

        widget::Column::new()
            .height(Length::Fill)
            .push_maybe(self.search_visible.then(|| self.view_search_bar()))
            .push(self.view_content())
            .into()
    }

    fn view_header_center(&self) -> Vec<Element<'_, EpubViewerMessage>> {
        vec![
            widget::text::heading(self.display_name())
                .wrapping(cosmic::iced::widget::text::Wrapping::None)
                .into(),
        ]
    }

    fn view_header_start(&self) -> Vec<Element<'_, EpubViewerMessage>> {
        let cosmic_theme::Spacing { space_xxs, .. } = theme::active().cosmic().spacing;

        vec![
            widget::button::icon(widget::icon::from_name("go-previous-symbolic").size(ICON_SIZE))
                .on_press(EpubViewerMessage::Out(EpubViewerOutput::Close(
                    self.fingerprint.clone(),
                    self.current_progress(),
                )))
                .tooltip(fl!("epub-viewer-back"))
                .padding(space_xxs)
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
            widget::button::icon(
                widget::icon::from_name("document-properties-symbolic").size(ICON_SIZE),
            )
            .on_press(EpubViewerMessage::Out(
                EpubViewerOutput::OpenDocumentDetails(Box::new(self.document.clone())),
            ))
            .tooltip(fl!("epub-viewer-document-details"))
            .padding(space_xxs)
            .into(),
        ]
    }

    /// @feature: app.epub_viewer_choice
    fn view_context(&self) -> ContextView<'_, EpubViewerMessage> {
        let open_in_section = widget::settings::section()
            .title(fl!("epub-viewer-open-in-section"))
            .add(
                widget::settings::item::builder(fl!("epub-viewer-open-in-mupdf")).control(
                    widget::button::standard(fl!("epub-viewer-open-in-action")).on_press(
                        EpubViewerMessage::Out(EpubViewerOutput::OpenInMuPdfViewer(Box::new(
                            self.document.clone(),
                        ))),
                    ),
                ),
            )
            .add(
                widget::settings::item::builder(fl!("epub-viewer-open-in-external")).control(
                    widget::button::standard(fl!("epub-viewer-open-in-action")).on_press(
                        EpubViewerMessage::Out(EpubViewerOutput::OpenInExternalViewer(Box::new(
                            self.document.clone(),
                        ))),
                    ),
                ),
            );

        let mut display_section = widget::settings::section().title(fl!("epub-viewer-display"));

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
                    ])
                    .width(Length::Shrink),
                ),
            );

            let margin_px = self.page_margin.round() as u32;
            display_section = display_section.add(
                widget::settings::item::builder(format!(
                    "{} ({}px)",
                    fl!("epub-viewer-page-margin"),
                    margin_px
                ))
                .control(
                    widget::slider(0.0..=128.0, self.page_margin, |v| {
                        EpubViewerMessage::SetPageMargin(v)
                    })
                    .step(4.0_f32),
                ),
            );
        }

        display_section = display_section.add(
            widget::settings::item::builder(fl!("epub-viewer-font")).control(
                FontPicker::new(
                    &self.font_picker_options,
                    fl!("epub-viewer-font"),
                    &self.font_picker_query,
                    EpubViewerMessage::FontPickerQuery,
                )
                .selected(self.font_family.label())
                .on_select(|s| EpubViewerMessage::SetFontFamily(str_to_font_family(&s)))
                .on_open(EpubViewerMessage::FontPickerOpen)
                .on_close(EpubViewerMessage::FontPickerClose)
                .focused(self.font_picker_focused)
                .view(),
            ),
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
                .step(1.0_f32),
            ),
        );

        let display_section = if self.debug {
            display_section.add(
                widget::settings::item::builder(fl!("epub-viewer-raw-html"))
                    .toggler(self.show_raw_html, EpubViewerMessage::ShowRawHtml),
            )
        } else {
            display_section
        };

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
        }
        .add(shortcut_item(
            "Ctrl+F",
            fl!("epub-viewer-shortcut-toggle-search"),
        ))
        .add(shortcut_item(
            "Escape",
            fl!("epub-viewer-shortcut-close-search"),
        ));

        ContextView {
            title: fl!("epub-viewer"),
            content: widget::settings::view_column(vec![
                open_in_section.into(),
                display_section.into(),
                shortcuts_section.into(),
            ])
            .into(),
        }
    }

    fn nav_tree(&self, active: bool) -> Option<read_flow_widgets::NavItem<EpubViewerMessage>> {
        use read_flow_widgets::NavItem;
        use read_flow_widgets::NavNode;

        if self.chapters.is_empty() && self.nav_entries.is_empty() {
            return None;
        }

        let children = self.build_nav_items();

        Some(NavItem::Node(NavNode {
            icon: Some(
                cosmic::widget::icon::from_name("application-epub+zip")
                    .size(ICON_SIZE)
                    .icon(),
            ),
            label: self.display_name(),
            active,
            collapsed: self.top_level_nav_collapsed,
            on_activate: EpubViewerMessage::Out(EpubViewerOutput::Activate),
            on_toggle: EpubViewerMessage::ToggleTopLevelNav,
            children,
            on_expand_all: Some(EpubViewerMessage::ExpandAllNavEntries),
            on_collapse_all: Some(EpubViewerMessage::CollapseAllNavEntries),
            on_close: Some(EpubViewerMessage::RequestClose),
        }))
    }

    fn update(&mut self, message: EpubViewerMessage) -> Task<Action<EpubViewerMessage>> {
        let task = self.handle_message(message);
        // Scroll positions looked up during view() read these heights, so
        // view() never lays blocks out itself.
        self.ensure_block_heights(self.active_chapter, SCROLL_MEASURE_WIDTH);
        task
    }
}

impl EpubViewer {
    fn handle_message(&mut self, message: EpubViewerMessage) -> Task<Action<EpubViewerMessage>> {
        self.maybe_repaginate();
        match message {
            EpubViewerMessage::EpubLoaded(title, chapters, epub_doc) => {
                self.title = title;
                self.chapters = chapters;
                self.block_heights_cache.clear();
                self.nav_entries = epub_doc
                    .as_ref()
                    .map(|d| d.as_ref().nav().to_vec())
                    .unwrap_or_default();
                self.epub_document = epub_doc;
                // Start with all sub-entries collapsed so only top-level chapters are visible.
                self.collapsed_nav_entries = self.all_parent_nav_indices();
                if !self.chapters.is_empty() {
                    self.active_chapter = self
                        .initial_chapter
                        .unwrap_or(0)
                        .min(self.chapters.len() - 1);
                    // If progress arrived before chapters, resolve the node path now.
                    if self.pending_block_index.is_none() && !self.pending_node_path.is_empty() {
                        let c = self.active_chapter;
                        let node_path = std::mem::take(&mut self.pending_node_path);
                        let block_idx = self
                            .chapters
                            .get(c)
                            .map(|ch| block_index_for_path(ch, &node_path))
                            .unwrap_or(0);
                        self.pending_block_index = Some(block_idx);
                    }
                }
                self.sync_raw_html_content();
                // Restore scroll position once content is available.
                // In scroll mode, prefer block_index over raw scroll_y so
                // the position is layout-independent (accurate across devices
                // with different display dimensions or font size settings).
                let target_y = if self.view_mode == ViewMode::Scroll {
                    if let Some(bi) = self.pending_block_index.take() {
                        let content_w = SCROLL_MEASURE_WIDTH;
                        let chapter_idx = self.active_chapter;
                        self.ensure_block_heights(chapter_idx, content_w);
                        self.block_heights_cache
                            .get(&chapter_idx)
                            .map(|(_, _, heights)| y_for_block_index_from_heights(heights, bi))
                    } else if self.scroll_y > 0.0 {
                        Some(self.scroll_y)
                    } else {
                        None
                    }
                } else {
                    None
                };

                if let Some(y) = target_y {
                    self.scroll_y = y;
                    scrollable::scroll_to(
                        self.content_scroll_id.clone(),
                        scrollable::AbsoluteOffset { x: 0.0, y }.into(),
                    )
                } else {
                    Task::none()
                }
            }
            EpubViewerMessage::ReadingProgressLoaded(pos) => {
                self.initial_chapter = pos.chapter;
                if !self.chapters.is_empty()
                    && let Some(c) = pos.chapter
                    && c < self.chapters.len()
                {
                    self.active_chapter = c;
                    self.sync_raw_html_content();
                    // Chapters already loaded — resolve node path to block index now.
                    let block_idx = self
                        .chapters
                        .get(c)
                        .map(|ch| block_index_for_path(ch, &pos.node_path))
                        .unwrap_or(0);
                    self.pending_block_index = Some(block_idx);
                    if self.view_mode == ViewMode::Scroll {
                        let content_w = SCROLL_MEASURE_WIDTH;
                        self.ensure_block_heights(c, content_w);
                        let target_y = self.block_heights_cache.get(&c).map(|(_, _, heights)| {
                            y_for_block_index_from_heights(heights, block_idx)
                        });
                        if let Some(y) = target_y {
                            self.scroll_y = y;
                            return scrollable::scroll_to(
                                self.content_scroll_id.clone(),
                                scrollable::AbsoluteOffset { x: 0.0, y }.into(),
                            );
                        }
                    }
                } else {
                    // Chapters not yet loaded — store for resolution in EpubLoaded.
                    self.pending_node_path = pos.node_path;
                }
                Task::none()
            }
            EpubViewerMessage::SelectChapter(idx) => {
                if idx < self.chapters.len() {
                    self.active_chapter = idx;
                    self.active_nav_entry = None;
                    self.scroll_y = 0.0;
                    self.current_page = 0;
                    self.highlighted_block = None;
                    self.search_matches.clear();
                    self.search_current = 0;
                    self.sync_raw_html_content();
                }
                Task::none()
            }
            EpubViewerMessage::SelectNavEntry(nav_idx) => {
                self.active_nav_entry = Some(nav_idx);
                let nav_activated: Task<Action<EpubViewerMessage>> = Task::none();
                if let Some(entry) = self.nav_entries.get(nav_idx) {
                    let (base, fragment) = match entry.href.split_once('#') {
                        Some((b, f)) => (b.to_owned(), Some(f.to_owned())),
                        None => (entry.href.clone(), None),
                    };
                    if let Some(idx) = self.chapters.iter().position(|c| c.href == base) {
                        self.active_chapter = idx;
                        self.scroll_y = 0.0;
                        self.current_page = 0;
                        self.highlighted_block = None;
                        self.search_matches.clear();
                        self.search_current = 0;
                        self.sync_raw_html_content();

                        // Navigate to the fragment position within the chapter.
                        if let Some(frag) = fragment.filter(|f| !f.is_empty()) {
                            // Compute block_idx and chapter length before calling
                            // ensure_block_heights (which needs &mut self).
                            let (block_idx, chapter_block_count) = {
                                let chapter = &self.chapters[idx];
                                let bidx = chapter.blocks.iter().position(
                                    |b| matches!(b, ContentBlock::Anchor { id } if id == &frag),
                                );
                                (bidx, chapter.blocks.len())
                            };

                            let mut nav_task = Task::none();

                            match self.view_mode {
                                ViewMode::Scroll => {
                                    let content_w = SCROLL_MEASURE_WIDTH;
                                    self.ensure_block_heights(idx, content_w);
                                    let target_y = if let Some((_, _, heights)) =
                                        self.block_heights_cache.get(&idx)
                                    {
                                        anchor_y_from_heights(
                                            &self.chapters[idx].blocks,
                                            heights,
                                            &frag,
                                        )
                                    } else {
                                        None
                                    };
                                    if let Some(target_y) = target_y {
                                        self.scroll_y = target_y;
                                        nav_task = scrollable::scroll_to(
                                            self.content_scroll_id.clone(),
                                            scrollable::AbsoluteOffset {
                                                x: 0.0,
                                                y: target_y,
                                            }
                                            .into(),
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
                                && bi + 1 < chapter_block_count
                            {
                                self.highlighted_block = Some(bi + 1);
                                let clear_task = Task::perform(
                                    async {
                                        tokio::time::sleep(std::time::Duration::from_millis(1500))
                                            .await;
                                    },
                                    |_| cosmic::action::app(EpubViewerMessage::ClearHighlight),
                                );
                                return Task::batch([nav_task, clear_task, nav_activated]);
                            }

                            return Task::batch([nav_task, nav_activated]);
                        }
                    }
                }
                nav_activated
            }
            EpubViewerMessage::ShowRawHtml(show) => {
                self.show_raw_html = show;
                self.sync_raw_html_content();
                Task::none()
            }
            EpubViewerMessage::RawHtmlAction(action) => {
                if !action.is_edit() {
                    self.raw_html_content.perform(action);
                }
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
                        // Ensure cached heights before borrowing chapter (avoid split borrow).
                        let chapter_idx = self.active_chapter;
                        let content_w = SCROLL_MEASURE_WIDTH;
                        self.ensure_block_heights(chapter_idx, content_w);

                        if let Some(chapter) = self.chapters.get(chapter_idx) {
                            tracing::info!(
                                "following fragment link #{frag} in chapter {}",
                                chapter.href
                            );
                            let target_y = self.block_heights_cache.get(&chapter_idx).and_then(
                                |(_, _, heights)| {
                                    anchor_y_from_heights(&chapter.blocks, heights, frag)
                                },
                            );
                            if target_y.is_none() {
                                tracing::info!(
                                    "anchor #{frag} not found in chapter {}",
                                    chapter.href
                                );
                            }
                            if let Some(target_y) = target_y {
                                if self.view_mode == ViewMode::Paginated {
                                    // Find the page containing the target block.
                                    // Works for both Footnote and Anchor targets.
                                    if let Some(block_idx) =
                                        chapter.blocks.iter().position(|b| match b {
                                            ContentBlock::Anchor { id } => id.as_str() == frag,
                                            ContentBlock::Footnote { id, .. } => {
                                                id.as_str() == frag
                                            }
                                            _ => false,
                                        })
                                        && let Some(layout) =
                                            self.pagination_cache.get(&chapter_idx)
                                        && let Some(page_idx) = layout
                                            .pages
                                            .iter()
                                            .position(|p| p.start <= block_idx && block_idx < p.end)
                                    {
                                        self.current_page = page_idx;
                                    }
                                    return Task::none();
                                }
                                return scrollable::scroll_to(
                                    self.content_scroll_id.clone(),
                                    scrollable::AbsoluteOffset {
                                        x: 0.0,
                                        y: target_y,
                                    }
                                    .into(),
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
                    tracing::info!(
                        "following cross-chapter link: {path} → {resolved} (from {})",
                        current.href
                    );
                    if let Some(idx) = self
                        .chapters
                        .iter()
                        .position(|c| c.href == resolved || c.href.ends_with(&resolved))
                    {
                        self.active_chapter = idx;
                        self.scroll_y = 0.0;
                        self.current_page = 0;
                        self.sync_raw_html_content();
                    } else if let Some(epub_doc) = &self.epub_document {
                        // Target is not in the spine (e.g. linear="no" item).
                        // Try to load it on-demand from the archive.
                        let doc = epub_doc.as_ref();
                        if let Ok(data) = doc.resolve_resource(&resolved) {
                            let raw_html = String::from_utf8_lossy(&data).into_owned();
                            let stylesheet = load_chapter_stylesheets(&raw_html, &resolved, doc);
                            let (mut blocks, block_paths) = epub::content::parse_xhtml_with_paths(
                                &data,
                                &resolved,
                                &stylesheet,
                                &mut |img_path| match doc.resolve_resource(img_path) {
                                    Ok(img_data) => {
                                        let media_type = epub::content::guess_media_type(img_path);
                                        Some((img_data, media_type))
                                    }
                                    Err(_) => None,
                                },
                            );
                            resolve_svg_blocks(&mut blocks, &resolved, doc);
                            let mut image_handles = HashMap::new();
                            collect_image_handles(&blocks, &mut image_handles);
                            self.chapters.push(EpubChapter {
                                label: resolved.clone(),
                                href: resolved.clone(),
                                blocks,
                                block_paths,
                                image_handles,
                                raw_html,
                            });
                            self.active_chapter = self.chapters.len() - 1;
                            self.scroll_y = 0.0;
                            self.current_page = 0;
                            self.sync_raw_html_content();
                        } else {
                            tracing::info!(
                                "cross-chapter link target not found among {} chapters \
                                 and not in archive: {resolved}",
                                self.chapters.len()
                            );
                        }
                    } else {
                        tracing::info!(
                            "cross-chapter link target not found among {} chapters: {resolved}",
                            self.chapters.len()
                        );
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
                            self.sync_raw_html_content();
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
                            self.sync_raw_html_content();
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
                self.save_current_prefs();
                Task::none()
            }
            EpubViewerMessage::NextPage => {
                self.maybe_repaginate();
                let total = self.total_pages();
                let step = self.page_step();
                if self.current_page + step < total {
                    self.current_page += step;
                } else if self.active_chapter + 1 < self.chapters.len() {
                    self.active_chapter += 1;
                    self.current_page = 0;
                    self.maybe_repaginate();
                    self.sync_raw_html_content();
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
                    // Land on the last spread, not the last single page, so
                    // dual-page mode doesn't show a lone final page unnecessarily.
                    let total = self.total_pages();
                    self.current_page = (total.saturating_sub(1) / step) * step;
                    self.sync_raw_html_content();
                }
                Task::none()
            }
            EpubViewerMessage::SetDualPage(mode) => {
                self.dual_page = mode;
                // Invalidate pagination cache since page width changes.
                self.pagination_cache.clear();
                self.maybe_repaginate();
                self.save_current_prefs();
                Task::none()
            }
            EpubViewerMessage::SetPageMargin(margin) => {
                self.page_margin = margin.clamp(0.0, 128.0);
                self.pagination_cache.clear();
                self.maybe_repaginate();
                self.save_current_prefs();
                Task::none()
            }
            EpubViewerMessage::SetFontFamily(family) => {
                self.font_family = family;
                self.font_picker_query = String::new();
                self.font_picker_focused = false;
                self.pagination_cache.clear();
                self.block_heights_cache.clear();
                self.maybe_repaginate();
                self.save_current_prefs();
                Task::none()
            }
            EpubViewerMessage::FontOptionsLoaded(options) => {
                self.font_picker_options = options;
                Task::none()
            }
            EpubViewerMessage::FontPickerOpen => {
                self.font_picker_focused = true;
                self.font_picker_query = String::new();
                Task::none()
            }
            EpubViewerMessage::FontPickerClose => {
                self.font_picker_focused = false;
                self.font_picker_query = String::new();
                Task::none()
            }
            EpubViewerMessage::FontPickerQuery(q) => {
                self.font_picker_query = q;
                Task::none()
            }
            EpubViewerMessage::SetBaseFontSize(size) => {
                self.base_font_size = size.clamp(12.0, 24.0);
                self.pagination_cache.clear();
                self.block_heights_cache.clear();
                self.maybe_repaginate();
                self.save_current_prefs();
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
            EpubViewerMessage::CopyCodeBlock(text) => cosmic::iced::clipboard::write(text),
            EpubViewerMessage::OpenImageViewer(image) => task::message(EpubViewerMessage::Out(
                EpubViewerOutput::OpenImageViewer(image),
            )),
            EpubViewerMessage::ToggleTopLevelNav => {
                self.top_level_nav_collapsed = !self.top_level_nav_collapsed;
                Task::none()
            }
            EpubViewerMessage::ToggleNavEntry(idx) => {
                if !self.collapsed_nav_entries.remove(&idx) {
                    self.collapsed_nav_entries.insert(idx);
                }
                Task::none()
            }
            EpubViewerMessage::ExpandAllNavEntries => {
                self.collapsed_nav_entries.clear();
                Task::none()
            }
            EpubViewerMessage::CollapseAllNavEntries => {
                self.collapsed_nav_entries = self.all_parent_nav_indices();
                Task::none()
            }
            EpubViewerMessage::LoadFailed(error) => {
                tracing::warn!("EPUB load failed: {error}");
                self.load_error = Some(error);
                Task::none()
            }
            EpubViewerMessage::RequestClose => {
                let progress = self.current_progress();
                task::message(EpubViewerMessage::Out(EpubViewerOutput::Close(
                    self.fingerprint.clone(),
                    progress,
                )))
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
/// Includes scroll offset, block index, view mode, and font size so that
/// the position can be accurately restored.  The format is backward-compatible:
/// older readers silently ignore unknown fields.
/// Build a CFI string for the given block in the given chapter.
fn cfi_for_block(chapters: &[EpubChapter], chapter_idx: usize, block_idx: usize) -> Option<String> {
    let chapter = chapters.get(chapter_idx)?;
    let node_path = chapter
        .block_paths
        .get(block_idx)
        .cloned()
        .unwrap_or_default();
    let locator = epub::Locator {
        spine_index: chapter_idx as u32,
        node_path,
        char_offset: 0,
    };
    Some(locator.to_cfi(None))
}

/// Find the flat block index in `chapter` whose stored node path best matches `target`.
/// "Best match" means the stored path is the longest prefix of `target`, giving
/// paragraph-level granularity for any CFI including text-node sub-offsets.
fn block_index_for_path(chapter: &EpubChapter, target: &[u32]) -> usize {
    if target.is_empty() {
        return 0;
    }
    chapter
        .block_paths
        .iter()
        .enumerate()
        .filter(|(_, p)| !p.is_empty() && target.starts_with(p.as_slice()))
        .max_by_key(|(_, p)| p.len())
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Serialize reading progress as an EPUB CFI position.
fn serialize_progress(
    chapters: &[EpubChapter],
    chapter_idx: usize,
    first_block: usize,
) -> (ViewerPosition, f64) {
    let cfi = cfi_for_block(chapters, chapter_idx, first_block)
        .unwrap_or_else(|| format!("epubcfi(/6/{}!:0)", (chapter_idx as u32 + 1) * 2));
    let position = ViewerPosition::Cfi(cfi);
    let total = chapters.len();
    let percentage = if total > 0 {
        (chapter_idx as f64 + 1.0) / total as f64
    } else {
        0.0
    };
    (position, percentage)
}

/// Parsed reading position from a CFI-based progress JSON string.
#[derive(Clone, Debug, Default)]
pub(crate) struct ReadingPosition {
    /// Parsed spine index from the CFI (used to select the chapter).
    chapter: Option<usize>,
    /// DOM node path extracted from the CFI; resolved to a block index once
    /// chapters are available.
    node_path: Vec<u32>,
}

/// Resolve a typed reading position (already extracted from the stored
/// envelope by `reading_progress::extract`) into the viewer's own restore
/// state.
fn resolve_reading_position(position: ViewerPosition) -> ReadingPosition {
    match position {
        ViewerPosition::Cfi(cfi) => match epub::Locator::from_cfi(&cfi) {
            Some(locator) => ReadingPosition {
                chapter: Some(locator.spine_index as usize),
                node_path: locator.node_path,
            },
            None => ReadingPosition::default(),
        },
        // `extract(_, Viewer::Epub)` never returns `Page` — that variant is MuPDF-only.
        ViewerPosition::Page(_) => ReadingPosition::default(),
    }
}

#[cfg(test)]
mod resolve_reading_position_tests {
    use super::*;

    #[test]
    fn resolves_cfi_format() {
        let pos = resolve_reading_position(ViewerPosition::Cfi("epubcfi(/6/4!/2:10)".to_string()));
        assert_eq!(pos.chapter, Some(1));
        assert_eq!(pos.node_path, vec![0]);
    }

    #[test]
    fn invalid_cfi_resolves_to_default() {
        let pos = resolve_reading_position(ViewerPosition::Cfi("not a cfi".to_string()));
        assert!(pos.chapter.is_none());
        assert!(pos.node_path.is_empty());
    }
}

/// Resolve embedded SVG image references in-place at chapter load time, so that
/// rendering doesn't need to read from the zip archive on every frame.
fn resolve_svg_blocks(blocks: &mut Vec<ContentBlock>, chapter_href: &str, doc: &EpubDocument) {
    for block in blocks {
        match block {
            ContentBlock::Svg { content, .. } => {
                *content =
                    epub::content::resolve_svg_images(content, chapter_href, &mut |img_path| {
                        match doc.resolve_resource(img_path) {
                            Ok(img_data) => {
                                let media_type = epub::content::guess_media_type(img_path);
                                Some((img_data, media_type))
                            }
                            Err(e) => {
                                tracing::info!(
                                    "SVG image resource not found in chapter \
                                 {chapter_href}: {img_path} ({e})"
                                );
                                None
                            }
                        }
                    });
            }
            ContentBlock::Figure { blocks, .. } => resolve_svg_blocks(blocks, chapter_href, doc),
            ContentBlock::BlockQuote { children } => {
                resolve_svg_blocks(children, chapter_href, doc)
            }
            ContentBlock::Footnote { blocks, .. } => resolve_svg_blocks(blocks, chapter_href, doc),
            _ => {}
        }
    }
}

/// Collect stable image handles keyed by data pointer, recursing into nested blocks.
fn collect_image_handles(blocks: &[ContentBlock], map: &mut HashMap<usize, widget::image::Handle>) {
    for block in blocks {
        match block {
            ContentBlock::Image { data, .. } if !data.is_empty() => {
                map.insert(
                    data.as_ptr() as usize,
                    widget::image::Handle::from_bytes(data.clone()),
                );
            }
            ContentBlock::Figure { blocks, .. } => collect_image_handles(blocks, map),
            ContentBlock::BlockQuote { children } => collect_image_handles(children, map),
            ContentBlock::Footnote { blocks, .. } => collect_image_handles(blocks, map),
            _ => {}
        }
    }
}

/// Load EPUB chapters from a file path. Runs on a blocking thread.
fn load_epub_chapters(
    path: &Path,
) -> Result<(String, Vec<EpubChapter>, EpubDocument), epub::EpubError> {
    let epub_doc = EpubDocument::open(path)?;

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

        let (blocks, block_paths, raw_html) = match epub_doc.resolve_resource(&item.href) {
            Ok(data) => {
                let href = &item.href;
                let raw = String::from_utf8_lossy(&data).into_owned();
                let stylesheet = load_chapter_stylesheets(&raw, href, &epub_doc);
                let (mut blocks, block_paths) = epub::content::parse_xhtml_with_paths(
                    &data,
                    href,
                    &stylesheet,
                    &mut |img_path| match epub_doc.resolve_resource(img_path) {
                        Ok(img_data) => {
                            let media_type = epub::content::guess_media_type(img_path);
                            Some((img_data, media_type))
                        }
                        Err(e) => {
                            tracing::info!(
                                "image resource not found in chapter {href}: {img_path} ({e})"
                            );
                            None
                        }
                    },
                );
                resolve_svg_blocks(&mut blocks, href, &epub_doc);
                (blocks, block_paths, raw)
            }
            Err(e) => {
                tracing::warn!("failed to resolve spine item {}: {e}", item.href);
                (Vec::new(), Vec::new(), String::new())
            }
        };

        let mut image_handles = HashMap::new();
        collect_image_handles(&blocks, &mut image_handles);
        chapters.push(EpubChapter {
            label,
            href: item.href.clone(),
            blocks,
            block_paths,
            image_handles,
            raw_html,
        });
    }

    Ok((title, chapters, epub_doc))
}

/// Extract `<link rel="stylesheet" href="...">` references and inline `<style>` blocks
/// from XHTML and load/parse them into a merged `StyleSheet`.
fn load_chapter_stylesheets(xhtml: &str, chapter_href: &str, doc: &EpubDocument) -> StyleSheet {
    let mut stylesheet = StyleSheet::empty();
    let base = epub::content::base_dir(chapter_href);

    // Extract linked external stylesheets: <link rel="stylesheet" href="..." />
    for segment in xhtml.split("<link") {
        let Some(end) = segment.find('>') else {
            continue;
        };
        let tag_content = &segment[..end];
        let lower = tag_content.to_ascii_lowercase();
        if !lower.contains("stylesheet") {
            continue;
        }
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

    // Extract inline <style> blocks from the <head>.
    let lower_xhtml = xhtml.to_ascii_lowercase();
    let mut search_start = 0;
    while let Some(tag_start) = lower_xhtml[search_start..].find("<style") {
        let abs_tag_start = search_start + tag_start;
        // Find the closing '>' of the opening tag (may have attributes like type="text/css")
        let Some(tag_end) = lower_xhtml[abs_tag_start..].find('>') else {
            break;
        };
        let content_start = abs_tag_start + tag_end + 1;
        let Some(close_pos) = lower_xhtml[content_start..].find("</style") else {
            break;
        };
        let css_text = &xhtml[content_start..content_start + close_pos];
        let sheet = epub::content::parse_css(css_text);
        stylesheet.merge(sheet);
        search_start = content_start + close_pos + "</style".len();
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

/// Width scroll mode positions are measured at: the widest the scroll
/// column gets.
const SCROLL_MEASURE_WIDTH: f32 = 800.0;

/// Compute the y-offset for `block_idx` using pre-computed `heights`.
fn y_for_block_index_from_heights(heights: &[f32], block_idx: usize) -> f32 {
    const SPACING: f32 = 8.0;
    heights.iter().take(block_idx).map(|&h| h + SPACING).sum()
}

/// Walk `blocks` with pre-computed `heights` and return the accumulated y-offset
/// of the first `Anchor` or `Footnote` whose `id` matches `anchor_id`.
fn anchor_y_from_heights(blocks: &[ContentBlock], heights: &[f32], anchor_id: &str) -> Option<f32> {
    const SPACING: f32 = 8.0;
    let mut y = 0.0f32;
    for (block, &h) in blocks.iter().zip(heights.iter()) {
        match block {
            ContentBlock::Anchor { id } if id == anchor_id => return Some(y),
            ContentBlock::Footnote { id, .. } if id == anchor_id => return Some(y),
            _ => {}
        }
        y += h + SPACING;
    }
    None
}

/// Return the byte offset in `text` corresponding to `char_offset` Unicode code points.
/// Returns `text.len()` when `char_offset >= text.chars().count()`.
fn char_offset_to_byte(text: &str, char_offset: usize) -> usize {
    text.char_indices()
        .nth(char_offset)
        .map(|(i, _)| i)
        .unwrap_or(text.len())
}

/// Extract the character range `[start_char .. end_char)` from a `TextSpan` slice,
/// preserving all span metadata (style, link, color, font_size_em).
fn slice_spans(spans: &[TextSpan], start_char: usize, end_char: usize) -> Vec<TextSpan> {
    if start_char >= end_char {
        return Vec::new();
    }
    let mut result = Vec::new();
    let mut pos = 0usize;
    for span in spans {
        let span_char_len = span.text.chars().count();
        let span_end = pos + span_char_len;
        if span_end <= start_char || pos >= end_char {
            pos = span_end;
            continue;
        }
        let local_start = start_char.saturating_sub(pos);
        let local_end = (end_char - pos).min(span_char_len);
        let byte_start = char_offset_to_byte(&span.text, local_start);
        let byte_end = char_offset_to_byte(&span.text, local_end);
        let sliced = span.text[byte_start..byte_end].to_string();
        if !sliced.is_empty() {
            result.push(TextSpan {
                text: sliced,
                ..span.clone()
            });
        }
        pos = span_end;
    }
    result
}

/// A paragraph's content as pagination measures it.
#[derive(Clone, Copy)]
struct ParagraphContent<'a> {
    text: &'a str,
    spans: &'a [TextSpan],
    style: &'a BlockStyle,
}

impl ParagraphContent<'_> {
    /// Lay out the paragraph from character `start` on, as a page showing it
    /// from there draws it.
    fn layout_from(
        self,
        start: usize,
        content_width: f32,
        font_size: f32,
        family: font::Family,
    ) -> Paragraph {
        let size = render::paragraph_font_size(self.style, font_size);
        let spans = render::owned_paragraph_spans(
            self.text[char_offset_to_byte(self.text, start)..].to_string(),
            slice_spans(self.spans, start, usize::MAX),
            family,
            size,
        );
        render::layout_paragraph(&spans, self.style, family, size, content_width)
    }

    /// Rendered height of the paragraph shown from character `start` on,
    /// including its margins.
    fn height_from(
        self,
        start: usize,
        content_width: f32,
        font_size: f32,
        family: font::Family,
    ) -> f32 {
        let (top, bottom) = render::block_margins(self.style);
        let layout = self.layout_from(start, content_width, font_size, family);
        layout.min_bounds().height + top + bottom
    }

    /// Character offset at which the paragraph shown from character `start`
    /// on must continue on the next page, so that the lines before it fit in
    /// `available_height` pixels. The offset is the start of the first line
    /// that does not fit, so every line on the page stays as full as the
    /// layout made it. Returns `None` when not even one line fits, or when all
    /// lines fit.
    fn split_from(
        self,
        start: usize,
        content_width: f32,
        font_size: f32,
        family: font::Family,
        available_height: f32,
    ) -> Option<usize> {
        let (top, bottom) = render::block_margins(self.style);
        let layout = self.layout_from(start, content_width, font_size, family);
        first_overflowing_char(&layout, available_height - top - bottom)
            .map(|offset| start + offset)
    }
}

/// Character offset in the text laid out as `layout` of the start of the first
/// line that does not fit in `available` pixels. Returns `None` when not even
/// one line fits, or when all lines fit.
fn first_overflowing_char(layout: &Paragraph, available: f32) -> Option<usize> {
    let buffer = layout.buffer();
    let mut fitting_lines = 0;
    let first_overflowing = buffer.layout_runs().find(|run| {
        let fits = run.line_top + run.line_height <= available;
        fitting_lines += usize::from(fits);
        !fits
    })?;
    if fitting_lines == 0 {
        return None;
    }
    // Character offset of the overflowing line's first glyph, counted over
    // the laid-out text (buffer lines plus their line endings).
    let preceding_lines: usize = buffer.lines[..first_overflowing.line_i]
        .iter()
        .map(|line| line.text().chars().count() + line.ending().as_str().chars().count())
        .sum();
    let line_text = buffer.lines[first_overflowing.line_i].text();
    let glyph_start = first_overflowing
        .glyphs
        .iter()
        .map(|glyph| glyph.start)
        .min()
        .unwrap_or(0);
    Some(preceding_lines + line_text[..glyph_start].chars().count())
}

/// Height of `block` drawn by `ctx` `content_width` pixels wide, shown from
/// `from` on (a block continued from the previous page).
fn effective_block_height(
    measurer: &mut BlockMeasurer,
    ctx: &RenderContext<'_>,
    block: &ContentBlock,
    from: BlockCut,
    content_width: f32,
) -> f32 {
    match block {
        _ if from == BlockCut::default() => block_height(measurer, ctx, block, content_width),
        ContentBlock::Paragraph { text, spans, style } => ParagraphContent { text, spans, style }
            .height_from(from.char_offset, content_width, ctx.font_size, ctx.family),
        _ => measurer.height(
            render_block_part(ctx, block, from, None, BlockHighlight::None),
            content_width,
        ),
    }
}

/// The part of `block` from `from` up to `to` (`None`: to its end), as `ctx`
/// draws it on a page.
fn render_block_part<'a>(
    ctx: &RenderContext<'a>,
    block: &'a ContentBlock,
    from: BlockCut,
    to: Option<BlockCut>,
    highlight: BlockHighlight,
) -> Element<'a, EpubViewerMessage> {
    if from == BlockCut::default() && to.is_none() {
        return ctx.render_block(block, highlight);
    }
    if let Some((items, ordered_start)) = list_parts(block) {
        return render::apply_highlight(
            render_partial_list(items, ordered_start, from, to, ctx.font_size, ctx.family),
            highlight,
        );
    }
    if render::container_children(block).is_some() {
        return render::apply_highlight(ctx.render_partial_container(block, from, to), highlight);
    }
    let end = to.map_or(usize::MAX, |cut| cut.char_offset);
    match block {
        ContentBlock::Paragraph { text, spans, style } => {
            let char_count = text.chars().count();
            let s = from.char_offset.min(char_count);
            let e = end.min(char_count);
            render_partial_paragraph(
                text[char_offset_to_byte(text, s)..char_offset_to_byte(text, e)].to_string(),
                slice_spans(spans, s, e),
                style,
                highlight,
                ctx.font_size,
                ctx.family,
            )
        }
        ContentBlock::Preformatted { text, spans, .. } => {
            let s = from.char_offset.min(text.chars().count());
            let e = preformatted_part_end(text, s, end);
            render_partial_preformatted(
                text[char_offset_to_byte(text, s)..char_offset_to_byte(text, e)].to_string(),
                slice_spans(spans, s, e),
                ctx.font_size,
                (s == 0).then(|| text.clone()),
            )
        }
        _ => ctx.render_block(block, highlight),
    }
}

/// The items of list `block`, and the number of its first item when it is
/// ordered; `None` when `block` is not a list.
fn list_parts(block: &ContentBlock) -> Option<(&[ListItem], Option<u32>)> {
    match block {
        ContentBlock::UnorderedList { items } => Some((items, None)),
        ContentBlock::OrderedList { start, items } => Some((items, Some(*start))),
        _ => None,
    }
}

/// The cut in container child `child` that a container's cut at
/// `char_offset` in that child stands for (see [`BlockCut`]).
fn container_child_cut(child: &ContentBlock, char_offset: usize) -> BlockCut {
    if let Some((items, _)) = list_parts(child) {
        let mut item_start = 0;
        for (index, item) in items.iter().enumerate() {
            let len = item.text.chars().count();
            if char_offset <= item_start + len {
                return BlockCut {
                    child: index,
                    char_offset: char_offset - item_start,
                };
            }
            item_start += len + 1;
        }
        return BlockCut {
            child: items.len(),
            char_offset: 0,
        };
    }
    if render::container_children(child).is_some() {
        return BlockCut {
            child: char_offset,
            char_offset: 0,
        };
    }
    BlockCut {
        child: 0,
        char_offset,
    }
}

/// The container cut's `char_offset` standing for cut `cut` in container
/// child `child`; the inverse of [`container_child_cut`]. A cut inside a
/// nested container's child is rounded back to the start of that child.
fn container_child_offset(child: &ContentBlock, cut: BlockCut) -> usize {
    if let Some((items, _)) = list_parts(child) {
        let preceding: usize = items[..cut.child.min(items.len())]
            .iter()
            .map(|item| item.text.chars().count() + 1)
            .sum();
        return preceding + cut.char_offset;
    }
    if render::container_children(child).is_some() {
        return cut.child;
    }
    cut.char_offset
}

/// Where to cut list `items` (numbered from `ordered_start` when ordered),
/// shown from `from` on, so that the items before the cut fit in
/// `available_height` pixels: between the items, or inside the first item
/// that does not fit, at the start of its first line that does not fit.
/// Returns `None` when all of it fits.
fn split_list(
    items: &[ListItem],
    ordered_start: Option<u32>,
    from: BlockCut,
    content_width: f32,
    font_size: f32,
    family: font::Family,
    available_height: f32,
) -> Option<BlockCut> {
    let mut used = 0.0;
    for (i, part) in render::list_item_parts(items, ordered_start, from, None)
        .iter()
        .enumerate()
    {
        let spans = render::list_item_part_spans(&items[part.index], part, family, font_size);
        let layout = render::layout_list_item(&spans, family, font_size, content_width);
        let spacing = if i > 0 {
            render::list_item_spacing()
        } else {
            0.0
        };
        let available = available_height - used - spacing;
        let height = layout.min_bounds().height;
        if height <= available {
            used += spacing + height;
            continue;
        }
        // The prefix is laid out before the item's text, on its first line.
        let prefix_len = part
            .prefix
            .as_ref()
            .map_or(0, |prefix| prefix.chars().count());
        let char_offset = first_overflowing_char(&layout, available)
            .map_or(0, |offset| part.start + offset.saturating_sub(prefix_len));
        return Some(BlockCut {
            child: part.index,
            char_offset,
        });
    }
    None
}

/// End of the part of code block `text` shown from character `start` up to
/// character `end`. A part ending at a line boundary leaves out the line
/// ending (`\r\n`, `\n` or `\r`), so it draws exactly its lines, without an
/// empty last one.
fn preformatted_part_end(text: &str, start: usize, end: usize) -> usize {
    let char_count = text.chars().count();
    let end = end.min(char_count);
    if end <= start || end == char_count {
        return end;
    }
    let shown = &text[..char_offset_to_byte(text, end)];
    let ending = ["\r\n", "\n", "\r"]
        .into_iter()
        .find(|ending| shown.ends_with(ending))
        .map_or(0, |ending| ending.len());
    (end - ending).max(start)
}

/// Where to cut container `block` (see [`render::container_children`]),
/// drawn by `ctx` and shown from `from` on, so that the part before the cut
/// fits in `available_height` pixels: between its children, or inside the
/// first child that does not fit, the way [`try_split_block`] cuts that
/// child. Returns `None` when all of it fits.
fn split_container(
    ctx: &RenderContext<'_>,
    measurer: &mut BlockMeasurer,
    block: &ContentBlock,
    from: BlockCut,
    content_width: f32,
    available_height: f32,
) -> Option<BlockCut> {
    let children = render::container_children(block)?;
    let padding = render::container_padding(block);
    let width = content_width - padding.x();
    let available = available_height - padding.y();
    let spacing = render::container_child_spacing();
    let mut used = 0.0;
    for (index, child) in children.iter().enumerate().skip(from.child) {
        let child_from = if index == from.child {
            container_child_cut(child, from.char_offset)
        } else {
            BlockCut::default()
        };
        let height = measurer.height(
            ctx.render_container_child(block, index, child_from, None),
            width,
        );
        let gap = if used > 0.0 && height > 0.0 {
            spacing
        } else {
            0.0
        };
        if used + gap + height <= available {
            used += gap + height;
            continue;
        }
        let child_ctx = match block {
            ContentBlock::Footnote { .. } => ctx.footnote_context(),
            _ => *ctx,
        };
        let child_cut = match (block, child) {
            (ContentBlock::Footnote { id, .. }, ContentBlock::Paragraph { text, spans, .. }) => {
                let start = child_from.char_offset;
                let spans = render::footnote_paragraph_spans(
                    id,
                    index,
                    text,
                    spans,
                    start,
                    usize::MAX,
                    ctx.family,
                    ctx.font_size,
                );
                // The prefix is laid out before the text, on its first line.
                let prefix_len = spans[0].text.chars().count();
                let layout =
                    render::layout_list_item(&spans, ctx.family, child_ctx.font_size, width);
                first_overflowing_char(&layout, available - used - gap).map(|offset| BlockCut {
                    child: 0,
                    char_offset: start + offset.saturating_sub(prefix_len),
                })
            }
            _ => try_split_block(
                &child_ctx,
                measurer,
                child,
                width,
                available - used - gap,
                child_from,
            ),
        };
        let char_offset = child_cut
            .map(|cut| container_child_offset(child, cut))
            .filter(|&offset| offset > container_child_offset(child, child_from))
            .unwrap_or(0);
        return Some(BlockCut {
            child: index,
            char_offset,
        });
    }
    let caption = ctx.figure_caption(block)?;
    let height = measurer.height(caption, width);
    let gap = if used > 0.0 && height > 0.0 {
        spacing
    } else {
        0.0
    };
    (used + gap + height > available).then_some(BlockCut {
        child: children.len(),
        char_offset: 0,
    })
}

/// Where a page holding only the first shown child of container `block`,
/// shown from `from` on, ends when more of the block follows; `None`
/// otherwise. When that child is itself a container, the page holds only its
/// first shown child. A child too tall for a page that cannot be cut there is
/// placed alone this way.
fn after_first_container_child(
    ctx: &RenderContext<'_>,
    block: &ContentBlock,
    from: BlockCut,
) -> Option<BlockCut> {
    let parts = |block: &ContentBlock| {
        render::container_children(block).map_or(0, <[ContentBlock]>::len)
            + usize::from(ctx.figure_caption(block).is_some())
    };
    let children = render::container_children(block)?;
    if let Some(child) = children.get(from.child)
        && render::container_children(child).is_some()
    {
        let first = container_child_cut(child, from.char_offset).child;
        if first + 1 < parts(child) {
            return Some(BlockCut {
                child: from.child,
                char_offset: first + 1,
            });
        }
    }
    (from.child + 1 < parts(block)).then_some(BlockCut {
        child: from.child + 1,
        char_offset: 0,
    })
}

/// Where to cut `block`, drawn by `ctx` and shown from `from` on, so that the
/// part before the cut fits in `available_height` pixels: at the start of the
/// first line that does not fit, or between the block's children (see
/// [`BlockCut`]). Returns `None` when the block cannot be cut there: it is
/// not splittable, not even one line fits, or all of it fits.
fn try_split_block(
    ctx: &RenderContext<'_>,
    measurer: &mut BlockMeasurer,
    block: &ContentBlock,
    content_width: f32,
    available_height: f32,
    from: BlockCut,
) -> Option<BlockCut> {
    let font_size = ctx.font_size;
    let family = ctx.family;
    if let Some((items, ordered_start)) = list_parts(block) {
        return split_list(
            items,
            ordered_start,
            from,
            content_width,
            font_size,
            family,
            available_height,
        )
        .filter(|cut| *cut > from);
    }
    if render::container_children(block).is_some() {
        return split_container(ctx, measurer, block, from, content_width, available_height)
            .filter(|cut| *cut > from);
    }
    let char_offset = match block {
        ContentBlock::Paragraph { text, spans, style } => ParagraphContent { text, spans, style }
            .split_from(
                from.char_offset,
                content_width,
                font_size,
                family,
                available_height,
            ),
        ContentBlock::Preformatted { text, spans, .. } => {
            let start = from.char_offset.min(text.chars().count());
            let padding = render::preformatted_padding();
            let spans = render::owned_paragraph_spans(
                text[char_offset_to_byte(text, start)..].to_string(),
                slice_spans(spans, start, usize::MAX),
                cosmic::font::mono().family,
                font_size,
            );
            let layout =
                render::layout_preformatted(&spans, font_size, content_width - padding.x());
            first_overflowing_char(&layout, available_height - padding.y())
                .map(|offset| start + offset)
        }
        _ => None,
    };
    char_offset.map(|char_offset| BlockCut {
        child: 0,
        char_offset,
    })
}

/// Height of the page number line under each page in paginated mode.
const PAGE_INDICATOR_HEIGHT: f32 = 17.0;
/// Id of the container holding a page's text, including its margins.
fn page_text_id() -> widget::Id {
    widget::Id::new("epub-page-text")
}
/// Widest a page's text column gets, including its margins.
const MAX_PAGE_WIDTH: f32 = 800.0;

/// Width and height of one page's text when the pages fill `area`: one page,
/// or two side by side `gap` apart, each with `margin` on all sides and the
/// page number line below.
fn page_text_size(area: (f32, f32), dual: bool, gap: f32, margin: f32) -> (f32, f32) {
    let (width, height) = area;
    let paper_width = if dual { (width - gap) / 2.0 } else { width };
    (
        (paper_width.min(MAX_PAGE_WIDTH) - margin * 2.0).max(1.0),
        (height - PAGE_INDICATOR_HEIGHT - margin * 2.0).max(1.0),
    )
}

/// Split a chapter's blocks into pages that fit within `page_height` pixels.
///
/// Paragraphs and code blocks that straddle a page boundary are split at the
/// start of the first line that does not fit (see [`first_overflowing_char`]).
/// Lists are split between items, or inside the first item that does not fit
/// at the start of its first line that does not fit. Block quotes, footnotes
/// and figures are split between their children, or inside the first child
/// that does not fit as that child splits; a nested container is split only
/// between its own children. A child taller than a page that cannot be cut
/// there gets a page of its own; for a nested container that is its first
/// shown child. Other block types that exceed `page_height` get their own
/// page unchanged.
///
/// Blocks are measured as `ctx` draws them `page_width` wide, `block_spacing`
/// apart; blocks measuring 0 take no spacing, as the page column leaves them
/// out.
fn paginate_blocks(
    blocks: &[ContentBlock],
    page_height: f32,
    page_width: f32,
    block_spacing: f32,
    ctx: &RenderContext<'_>,
    measurer: &mut BlockMeasurer,
) -> PaginationLayout {
    let block_spacing = block_spacing.max(0.0);

    let mut pages: Vec<PageRange> = Vec::new();
    let mut page_start = 0usize;
    let mut page_start_cut = BlockCut::default();
    let mut accumulated = 0.0f32;
    let mut i = 0usize;

    while i < blocks.len() {
        let block = &blocks[i];
        let is_first_on_page = i == page_start;
        let from = if is_first_on_page {
            page_start_cut
        } else {
            BlockCut::default()
        };
        let block_h = effective_block_height(measurer, ctx, block, from, page_width);
        let spacing = if accumulated > 0.0 && block_h > 0.0 {
            block_spacing
        } else {
            0.0
        };

        if accumulated + spacing + block_h <= page_height {
            // Block fits on the current page.
            accumulated += spacing + block_h;
            i += 1;
            continue;
        }
        // Block doesn't fit — try to place its top part on the current page.
        let available = (page_height - accumulated - spacing).max(0.0);
        let cut = try_split_block(ctx, measurer, block, page_width, available, from)
            .filter(|cut| *cut > from)
            .or_else(|| {
                // A container's first shown child (or a nested container's
                // first shown child) that cannot be split is placed alone, to
                // overflow the page on its own.
                is_first_on_page
                    .then(|| after_first_container_child(ctx, block, from))
                    .flatten()
            });
        match cut {
            Some(cut) => {
                pages.push(PageRange {
                    start: page_start,
                    start_cut: page_start_cut,
                    end: i + 1,
                    end_cut: Some(cut),
                });
                page_start = i;
                page_start_cut = cut;
                // Re-process block `i` on the new page.
            }
            _ if is_first_on_page => {
                // Cannot split — let it overflow rather than loop forever.
                pages.push(PageRange {
                    start: page_start,
                    start_cut: page_start_cut,
                    end: i + 1,
                    end_cut: None,
                });
                page_start = i + 1;
                page_start_cut = BlockCut::default();
                i += 1;
            }
            _ => {
                // Nothing fits on this page — close it and start block on next page.
                pages.push(PageRange {
                    start: page_start,
                    start_cut: page_start_cut,
                    end: i,
                    end_cut: None,
                });
                page_start = i;
                page_start_cut = BlockCut::default();
                // Re-process block `i` on the new page.
            }
        }
        accumulated = 0.0;
    }

    // Final page (may be a continuation of a split block).
    if page_start < blocks.len() {
        pages.push(PageRange {
            start: page_start,
            start_cut: page_start_cut,
            end: blocks.len(),
            end_cut: None,
        });
    }

    if pages.is_empty() {
        pages.push(PageRange {
            start: 0,
            start_cut: BlockCut::default(),
            end: 0,
            end_cut: None,
        });
    }

    PaginationLayout {
        page_height,
        page_width,
        pages,
    }
}

/// Recursively convert a depth-annotated flat slice of nav entries into a
/// properly nested `NavItem` tree.
///
/// `entries` is a slice of `(original_index, &NavEntry)` pairs, all at depth
/// >= `depth`.  Entries at `depth` become top-level items; deeper entries
/// > become their children.
fn build_nav_items_at_depth(
    entries: &[(usize, &NavEntry)],
    depth: usize,
    collapsed: &std::collections::HashSet<usize>,
    active: Option<usize>,
) -> Vec<read_flow_widgets::NavItem<EpubViewerMessage>> {
    use read_flow_widgets::NavItem;
    use read_flow_widgets::NavLeaf;
    use read_flow_widgets::NavNode;

    let mut result = Vec::new();
    let mut i = 0;

    while i < entries.len() {
        let (orig_idx, entry) = entries[i];

        if entry.depth < depth {
            break;
        }
        if entry.depth > depth {
            i += 1;
            continue;
        }

        // Count how many subsequent entries belong to this item as children
        // (all entries with depth > current depth, before the next entry at
        // the same depth or shallower).
        let children_start = i + 1;
        let children_count = entries[children_start..]
            .iter()
            .take_while(|(_, e)| e.depth > depth)
            .count();
        let children_end = children_start + children_count;

        let children_slice = &entries[children_start..children_end];
        let children = if children_slice.is_empty() {
            vec![]
        } else {
            build_nav_items_at_depth(children_slice, depth + 1, collapsed, active)
        };

        let is_active = active == Some(orig_idx);

        if children.is_empty() {
            result.push(NavItem::Leaf(NavLeaf {
                icon: None,
                label: entry.label.clone(),
                active: is_active,
                on_activate: EpubViewerMessage::SelectNavEntry(orig_idx),
                on_close: None,
            }));
        } else {
            result.push(NavItem::Node(NavNode {
                icon: None,
                label: entry.label.clone(),
                active: is_active,
                collapsed: collapsed.contains(&orig_idx),
                on_activate: EpubViewerMessage::SelectNavEntry(orig_idx),
                on_toggle: EpubViewerMessage::ToggleNavEntry(orig_idx),
                children,
                on_expand_all: None,
                on_collapse_all: None,
                on_close: None,
            }));
        }

        i = children_end;
    }

    result
}

impl EpubViewer {
    /// Returns the indices of all nav entries that have at least one child entry,
    /// i.e. entries that can be collapsed.
    fn all_parent_nav_indices(&self) -> std::collections::HashSet<usize> {
        self.nav_entries
            .iter()
            .enumerate()
            .filter(|(i, entry)| {
                self.nav_entries
                    .get(i + 1)
                    .is_some_and(|next| next.depth > entry.depth)
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Build a nested `NavItem` tree from the flat `nav_entries` list (or fall
    /// back to the chapter spine when there are no nav entries).
    fn build_nav_items(&self) -> Vec<read_flow_widgets::NavItem<EpubViewerMessage>> {
        if !self.nav_entries.is_empty() {
            let indexed: Vec<(usize, &NavEntry)> = self.nav_entries.iter().enumerate().collect();
            build_nav_items_at_depth(
                &indexed,
                0,
                &self.collapsed_nav_entries,
                self.active_nav_entry,
            )
        } else {
            self.chapters
                .iter()
                .enumerate()
                .map(|(idx, chapter)| {
                    read_flow_widgets::NavItem::Leaf(read_flow_widgets::NavLeaf {
                        icon: None,
                        label: chapter.label.clone(),
                        active: idx == self.active_chapter,
                        on_activate: EpubViewerMessage::SelectChapter(idx),
                        on_close: None,
                    })
                })
                .collect()
        }
    }

    /// How scroll mode draws `chapter`'s blocks.
    fn scroll_render_context<'a>(&self, chapter: &'a EpubChapter) -> RenderContext<'a> {
        RenderContext {
            font_size: self.base_font_size,
            family: self.font_family.to_family(),
            max_image_height: f32::MAX,
            image_handles: &chapter.image_handles,
        }
    }

    /// How paginated mode draws `chapter`'s blocks on pages with
    /// `page_height` pixels of text.
    fn page_render_context<'a>(
        &self,
        chapter: &'a EpubChapter,
        page_height: f32,
    ) -> RenderContext<'a> {
        RenderContext {
            font_size: self.base_font_size,
            family: self.font_family.to_family(),
            max_image_height: page_height * 0.9,
            image_handles: &chapter.image_handles,
        }
    }

    /// Height of `block` of `chapter` as scroll mode lays it out `width`
    /// pixels wide.
    fn scroll_block_height(&self, chapter: &EpubChapter, block: &ContentBlock, width: f32) -> f32 {
        block_height(
            &mut self.measurer.borrow_mut(),
            &self.scroll_render_context(chapter),
            block,
            width,
        )
    }

    /// Ensure `block_heights_cache` has a valid entry for `chapter_idx` at the
    /// given `content_width`.  If the cached entry exists and matches both the
    /// width and the current `base_font_size` it is reused; otherwise the
    /// blocks are measured as scroll mode lays them out and stored.
    fn ensure_block_heights(&mut self, chapter_idx: usize, content_width: f32) {
        let font_size = self.base_font_size;
        if let Some((cw, fs, _)) = self.block_heights_cache.get(&chapter_idx)
            && (*cw - content_width).abs() < 0.5
            && (*fs - font_size).abs() < 0.001
        {
            return;
        }
        let heights = match self.chapters.get(chapter_idx) {
            Some(ch) => ch
                .blocks
                .iter()
                .map(|block| self.scroll_block_height(ch, block, content_width))
                .collect(),
            None => Vec::new(),
        };
        self.block_heights_cache
            .insert(chapter_idx, (content_width, font_size, heights));
    }

    fn sync_raw_html_content(&mut self) {
        if !self.show_raw_html {
            return;
        }
        self.raw_html_content = self
            .chapters
            .get(self.active_chapter)
            .map(|ch| text_editor::Content::with_text(&ch.raw_html))
            .unwrap_or_default();
    }

    /// Re-paginate the active chapter if the viewport size has changed.
    fn maybe_repaginate(&mut self) {
        if self.view_mode != ViewMode::Paginated {
            return;
        }
        let (vw, _) = self.viewport_size.get();
        let area = self.page_area.get();
        if vw <= 0.0 || area.0 <= 0.0 || area.1 <= 0.0 {
            return;
        }

        let space_xxs = theme::active().cosmic().spacing.space_xxs as f32;
        let (content_width, content_height) =
            page_text_size(area, self.should_dual_page(vw), space_xxs, self.page_margin);

        let needs_recompute = match self.pagination_cache.get(&self.active_chapter) {
            Some(cached) => {
                (cached.page_height - content_height).abs() > 1.0
                    || (cached.page_width - content_width).abs() > 1.0
            }
            None => true,
        };

        if needs_recompute && let Some(chapter) = self.chapters.get(self.active_chapter) {
            let layout = paginate_blocks(
                &chapter.blocks,
                content_height,
                content_width,
                space_xxs,
                &self.page_render_context(chapter, content_height),
                self.measurer.get_mut(),
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
            && let Some(block_idx) = self.approximate_block_at_scroll_y(self.scroll_y)
        {
            return layout
                .pages
                .iter()
                .position(|p| p.start <= block_idx && block_idx < p.end)
                .unwrap_or(0);
        }
        0
    }

    /// Convert current page index to approximate scroll_y position.
    fn page_to_scroll_y(&self) -> f32 {
        if let Some(layout) = self.pagination_cache.get(&self.active_chapter)
            && let Some(page) = layout.pages.get(self.current_page)
            && let Some(heights) = self.active_scroll_heights()
        {
            return y_for_block_index_from_heights(heights, page.start);
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

    /// Current reading position (EPUB CFI) and completion percentage, if a
    /// chapter has loaded. `None` while the document is still opening.
    pub(super) fn current_progress(&self) -> Option<ReadingProgress> {
        if self.chapters.is_empty() {
            return None;
        }
        let first_block = match self.view_mode {
            ViewMode::Paginated => self
                .pagination_cache
                .get(&self.active_chapter)
                .and_then(|l| l.pages.get(self.current_page))
                .map(|p| p.start)
                .unwrap_or(0),
            ViewMode::Scroll => self
                .approximate_block_at_scroll_y(self.scroll_y)
                .unwrap_or(0),
        };
        let (position, percentage) =
            serialize_progress(&self.chapters, self.active_chapter, first_block);
        Some(ReadingProgress {
            position,
            percentage,
        })
    }

    /// Scroll mode heights of the active chapter's blocks, as `update()` last
    /// measured them. `None` before the chapter has been measured.
    fn active_scroll_heights(&self) -> Option<&[f32]> {
        self.block_heights_cache
            .get(&self.active_chapter)
            .map(|(_, _, heights)| heights.as_slice())
    }

    /// Find the block index at a given scroll_y offset from the cached scroll
    /// mode heights, so it is cheap enough to call while drawing.
    fn approximate_block_at_scroll_y(&self, target_y: f32) -> Option<usize> {
        let heights = self.active_scroll_heights()?;
        let mut y = 0.0f32;
        for (i, &h) in heights.iter().enumerate() {
            if y + h > target_y {
                return Some(i);
            }
            y += h + 8.0;
        }
        Some(heights.len().saturating_sub(1))
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
                // Measure the y-position of the target block.
                let content_w = SCROLL_MEASURE_WIDTH;
                let chapter_idx = self.active_chapter;
                self.ensure_block_heights(chapter_idx, content_w);
                let y = self
                    .block_heights_cache
                    .get(&chapter_idx)
                    .map(|(_, _, heights)| y_for_block_index_from_heights(heights, block_idx))
                    .unwrap_or(0.0);
                self.scroll_y = y;
                scrollable::scroll_to(
                    self.content_scroll_id.clone(),
                    scrollable::AbsoluteOffset { x: 0.0, y }.into(),
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

#[cfg(test)]
fn render_chapter_blocks(
    chapters: &[EpubChapter],
    chapter_index: usize,
    font_size: f32,
) -> cosmic::Element<'_, EpubViewerMessage> {
    let chapter = &chapters[chapter_index];
    let ctx = render::RenderContext {
        font_size,
        family: font::Family::Serif,
        max_image_height: 400.0,
        image_handles: &chapter.image_handles,
    };
    let mut col = cosmic::widget::column::with_capacity(chapter.blocks.len())
        .spacing(8)
        .width(Length::Fill);
    for block in &chapter.blocks {
        col = col.push(ctx.render_block(block, BlockHighlight::None));
    }
    cosmic::widget::container(col)
        .padding(8)
        .width(Length::Fill)
        .into()
}

#[cfg(test)]
mod tests {
    use cosmic_golden::golden_test;

    use super::EpubViewerMessage;
    use super::EpubViewerOutput;
    use super::Page;
    use super::ViewerPosition;
    use super::load_epub_chapters;
    use super::render_chapter_blocks;
    use super::test_helper::EpubBuilder;

    #[tokio::test]
    async fn current_progress_is_none_before_chapters_load() {
        let (_app, document_provider, _db_dir) = crate::test_support::document_provider().await;
        let document = crate::aggregator::Document {
            document_guid: "doc-guid".into(),
            document_meta: Default::default(),
            contents: Vec::new(),
        };
        let (viewer, _init) = super::EpubViewer::new(document, document_provider, false);
        assert_eq!(viewer.current_progress(), None);
    }

    /// Scroll progress is looked up while drawing, so it must come from the
    /// heights `update()` cached rather than from laying blocks out again.
    #[tokio::test]
    async fn scroll_progress_reads_cached_block_heights() {
        let (application_module, document_provider, _db_dir) =
            crate::test_support::document_provider().await;
        let epub_file = EpubBuilder::new("Test")
            .body("<p>One</p><p>Two</p><p>Three</p><p>Four</p>")
            .build();
        let (document, _fixture_dir) = crate::test_support::scan_and_fetch_document(
            &application_module,
            &document_provider,
            epub_file.path().to_path_buf(),
            "test.epub",
        )
        .await;
        let (mut viewer, init_task) = super::EpubViewer::new(document, document_provider, false);
        for message in crate::test_support::drain(init_task).await {
            if matches!(message, super::EpubViewerMessage::EpubLoaded(..)) {
                let _ = viewer.update(message);
            }
        }
        viewer.view_mode = super::ViewMode::Scroll;
        let chapter = viewer.active_chapter;
        assert_eq!(
            viewer.block_heights_cache[&chapter].2.len(),
            viewer.chapters[chapter].blocks.len(),
            "update() measures the active chapter"
        );

        // Heights no layout would produce: only a cache lookup finds block 2.
        viewer
            .block_heights_cache
            .get_mut(&chapter)
            .expect("cached")
            .2 = vec![1000.0; viewer.chapters[chapter].blocks.len()];
        viewer.scroll_y = 2500.0;
        assert_eq!(
            viewer.approximate_block_at_scroll_y(viewer.scroll_y),
            Some(2)
        );
        let progress = viewer.current_progress().expect("chapters loaded");
        let (expected, _) = super::serialize_progress(&viewer.chapters, chapter, 2);
        assert_eq!(progress.position, expected);
    }

    #[tokio::test]
    async fn current_progress_reflects_the_loaded_chapter() {
        let (application_module, document_provider, _db_dir) =
            crate::test_support::document_provider().await;
        let epub_file = EpubBuilder::new("Test").body("<p>Hello</p>").build();
        let (document, _fixture_dir) = crate::test_support::scan_and_fetch_document(
            &application_module,
            &document_provider,
            epub_file.path().to_path_buf(),
            "test.epub",
        )
        .await;

        let (mut viewer, init_task) = super::EpubViewer::new(document, document_provider, false);
        for message in crate::test_support::drain(init_task).await {
            if matches!(message, super::EpubViewerMessage::EpubLoaded(..)) {
                let _ = viewer.update(message);
            }
        }

        let progress = viewer.current_progress().expect("chapters loaded");
        assert!(
            matches!(progress.position, ViewerPosition::Cfi(_)),
            "expected a CFI position, got {:?}",
            progress.position
        );
        assert!(progress.percentage > 0.0, "expected a nonzero percentage");
    }

    #[tokio::test]
    async fn nav_tree_close_button_carries_current_progress() {
        let (application_module, document_provider, _db_dir) =
            crate::test_support::document_provider().await;
        let epub_file = EpubBuilder::new("Test").body("<p>Hello</p>").build();
        let (document, _fixture_dir) = crate::test_support::scan_and_fetch_document(
            &application_module,
            &document_provider,
            epub_file.path().to_path_buf(),
            "test.epub",
        )
        .await;
        let fingerprint = document
            .contents
            .first()
            .map(|c| c.fingerprint.clone())
            .unwrap_or_default();

        let (mut viewer, init_task) = super::EpubViewer::new(document, document_provider, false);
        for message in crate::test_support::drain(init_task).await {
            let _ = viewer.update(message);
        }

        let Some(read_flow_widgets::NavItem::Node(node)) = viewer.nav_tree(false) else {
            panic!("expected a Node once chapters are loaded");
        };
        assert!(
            matches!(node.on_close, Some(EpubViewerMessage::RequestClose)),
            "nav_tree should wire the close button to RequestClose, not eagerly compute progress"
        );

        let task = viewer.update(EpubViewerMessage::RequestClose);
        let messages = crate::test_support::drain(task).await;
        let Some(EpubViewerMessage::Out(EpubViewerOutput::Close(closed_fingerprint, _))) =
            messages.into_iter().next()
        else {
            panic!("expected handling RequestClose to emit EpubViewerOutput::Close");
        };
        assert_eq!(closed_fingerprint, fingerprint);
    }

    #[golden_test(600, 150)]
    fn epub_plain_paragraph() -> cosmic::Element<'_, super::EpubViewerMessage> {
        let _guard = crate::test_support::cosmic_tk_lock();
        let _f = EpubBuilder::new("Test")
            .body("<p>This is a plain paragraph of body text.</p>")
            .build();
        let (_, chapters, _) = load_epub_chapters(_f.path()).unwrap();
        render_chapter_blocks(&chapters, 0, 16.0)
    }

    #[golden_test(600, 150, dark)]
    fn epub_plain_paragraph_dark() -> cosmic::Element<'_, super::EpubViewerMessage> {
        let _guard = crate::test_support::cosmic_tk_lock();
        let _f = EpubBuilder::new("Test")
            .body("<p>This is a plain paragraph of body text.</p>")
            .build();
        let (_, chapters, _) = load_epub_chapters(_f.path()).unwrap();
        render_chapter_blocks(&chapters, 0, 16.0)
    }

    #[golden_test(600, 300)]
    fn epub_headings() -> cosmic::Element<'_, super::EpubViewerMessage> {
        let _guard = crate::test_support::cosmic_tk_lock();
        let _f = EpubBuilder::new("Headings")
            .body(
                "<h1>Chapter One</h1>\
                 <h2>Section 1.1</h2>\
                 <h3>Subsection</h3>\
                 <p>Opening paragraph.</p>",
            )
            .build();
        let (_, chapters, _) = load_epub_chapters(_f.path()).unwrap();
        render_chapter_blocks(&chapters, 0, 16.0)
    }

    #[golden_test(600, 250)]
    fn epub_unordered_list() -> cosmic::Element<'_, super::EpubViewerMessage> {
        let _guard = crate::test_support::cosmic_tk_lock();
        let _f = EpubBuilder::new("List")
            .body(
                "<ul>\
                   <li>First item</li>\
                   <li>Second item</li>\
                   <li>Third with <strong>bold</strong> text</li>\
                 </ul>",
            )
            .build();
        let (_, chapters, _) = load_epub_chapters(_f.path()).unwrap();
        render_chapter_blocks(&chapters, 0, 16.0)
    }

    #[golden_test(600, 250)]
    fn epub_ordered_list() -> cosmic::Element<'_, super::EpubViewerMessage> {
        let _guard = crate::test_support::cosmic_tk_lock();
        let _f = EpubBuilder::new("Ordered List")
            .body(
                "<ol>\
                   <li>First step</li>\
                   <li>Second step</li>\
                   <li>Third with <em>italic</em> text</li>\
                 </ol>",
            )
            .build();
        let (_, chapters, _) = load_epub_chapters(_f.path()).unwrap();
        render_chapter_blocks(&chapters, 0, 16.0)
    }

    #[golden_test(600, 150)]
    fn epub_inline_styles() -> cosmic::Element<'_, super::EpubViewerMessage> {
        let _guard = crate::test_support::cosmic_tk_lock();
        let _f = EpubBuilder::new("Inline Styles")
            .body(
                "<p>\
                   Normal, <strong>bold</strong>, <em>italic</em>, \
                   <u>underline</u>, <del>strikethrough</del>, \
                   <code>monospaced</code> and \
                   <a href=\"chapter.xhtml\">a link</a>.\
                 </p>",
            )
            .build();
        let (_, chapters, _) = load_epub_chapters(_f.path()).unwrap();
        render_chapter_blocks(&chapters, 0, 16.0)
    }

    #[golden_test(600, 200)]
    fn epub_preformatted() -> cosmic::Element<'_, super::EpubViewerMessage> {
        let _guard = crate::test_support::cosmic_tk_lock();
        let _f = EpubBuilder::new("Preformatted")
            .body("<pre>fn hello() {\n    println!(\"Hello, world!\");\n}</pre>")
            .build();
        let (_, chapters, _) = load_epub_chapters(_f.path()).unwrap();
        render_chapter_blocks(&chapters, 0, 16.0)
    }

    #[golden_test(600, 200)]
    fn epub_blockquote() -> cosmic::Element<'_, super::EpubViewerMessage> {
        let _guard = crate::test_support::cosmic_tk_lock();
        let _f = EpubBuilder::new("Blockquote")
            .body(
                "<blockquote>\
                   <p>To be, or not to be, that is the question.</p>\
                   <p>— William Shakespeare</p>\
                 </blockquote>",
            )
            .build();
        let (_, chapters, _) = load_epub_chapters(_f.path()).unwrap();
        render_chapter_blocks(&chapters, 0, 16.0)
    }

    #[golden_test(600, 200)]
    fn epub_table() -> cosmic::Element<'_, super::EpubViewerMessage> {
        let _guard = crate::test_support::cosmic_tk_lock();
        let _f = EpubBuilder::new("Table")
            .body(
                "<table>\
                   <tr><th>Name</th><th>Age</th><th>City</th></tr>\
                   <tr><td>Alice</td><td>30</td><td>Amsterdam</td></tr>\
                   <tr><td>Bob</td><td>25</td><td>Berlin</td></tr>\
                 </table>",
            )
            .build();
        let (_, chapters, _) = load_epub_chapters(_f.path()).unwrap();
        render_chapter_blocks(&chapters, 0, 16.0)
    }

    #[golden_test(600, 100)]
    fn epub_horizontal_rule() -> cosmic::Element<'_, super::EpubViewerMessage> {
        let _guard = crate::test_support::cosmic_tk_lock();
        let _f = EpubBuilder::new("HR")
            .body("<p>Before the rule.</p><hr/><p>After the rule.</p>")
            .build();
        let (_, chapters, _) = load_epub_chapters(_f.path()).unwrap();
        render_chapter_blocks(&chapters, 0, 16.0)
    }

    #[golden_test(600, 200)]
    fn epub_footnote() -> cosmic::Element<'_, super::EpubViewerMessage> {
        let _guard = crate::test_support::cosmic_tk_lock();
        let _f = EpubBuilder::new("Footnote")
            .body(
                "<p>Main text with a reference<sup><a href=\"#fn1\">1</a></sup>.</p>\
                 <aside epub:type=\"footnote\" id=\"fn1\">\
                   <p>This is the footnote content explaining the reference.</p>\
                 </aside>",
            )
            .build();
        let (_, chapters, _) = load_epub_chapters(_f.path()).unwrap();
        render_chapter_blocks(&chapters, 0, 16.0)
    }

    #[golden_test(600, 250)]
    fn epub_footnote_styled_spans() -> cosmic::Element<'_, super::EpubViewerMessage> {
        let _guard = crate::test_support::cosmic_tk_lock();
        // Footnotes render with a reduced context font size (0.8×). Spans with
        // explicit em font sizes must scale against that reduced size, not the
        // hardcoded 16px base. This test exercises a footnote whose content
        // contains a span with font-size: 1.5em — it should appear proportionally
        // smaller than 1.5 × 16px would be in normal body text.
        let _f = EpubBuilder::new("Footnote Styled Spans")
            .body(
                "<p>Main text with <span style=\"font-size: 1.5em\">large styled text</span> and normal text<sup><a href=\"#fn1\">1</a></sup>.</p>\
                 <aside epub:type=\"footnote\" id=\"fn1\">\
                   <p>Footnote with <span style=\"font-size: 1.5em\">large styled text</span> and normal text.</p>\
                 </aside>",
            )
            .build();
        let (_, chapters, _) = load_epub_chapters(_f.path()).unwrap();
        render_chapter_blocks(&chapters, 0, 16.0)
    }

    #[golden_test(600, 300)]
    fn epub_figure() -> cosmic::Element<'_, super::EpubViewerMessage> {
        let _guard = crate::test_support::cosmic_tk_lock();
        // Use a small inline SVG as the figure content so no external resource
        // is needed while still exercising the Figure + caption render path.
        let _f = EpubBuilder::new("Figure")
            .body(
                "<figure>\
                   <svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 60\">\
                     <rect width=\"100\" height=\"60\" fill=\"#4080c0\"/>\
                     <polygon points=\"50,10 80,50 20,50\" fill=\"white\"/>\
                   </svg>\
                   <figcaption>Figure 1: A simple coloured rectangle.</figcaption>\
                 </figure>",
            )
            .build();
        let (_, chapters, _) = load_epub_chapters(_f.path()).unwrap();
        render_chapter_blocks(&chapters, 0, 16.0)
    }

    /// Minimal 8×8 orange PNG used by the image tests.
    const TINY_PNG: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, // PNG signature
        0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, // IHDR length + type
        0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x08, // 8×8 dimensions
        0x08, 0x02, 0x00, 0x00, 0x00, 0x4b, 0x6d, 0x29, // 8-bit RGB, CRC…
        0xdc, 0x00, 0x00, 0x00, 0x11, 0x49, 0x44, 0x41, // IDAT length + type
        0x54, 0x78, 0x9c, 0x63, 0x38, 0x91, 0x62, 0x84, // compressed data
        0x15, 0x31, 0x0c, 0x2d, 0x09, 0x00, 0x56, 0x46, //   (8×8 orange pixels)
        0x57, 0x81, 0x3c, 0xbd, 0x9f, 0x89, 0x00, 0x00, //
        0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, // IEND
        0x60, 0x82,
    ];

    #[golden_test(600, 200)]
    fn epub_image() -> cosmic::Element<'_, super::EpubViewerMessage> {
        let _guard = crate::test_support::cosmic_tk_lock();
        let _f = EpubBuilder::new("Image")
            .body("<p>An image follows:</p><img src=\"images/test.png\" alt=\"Test image\"/>")
            .resource("OEBPS/images/test.png", TINY_PNG.to_vec(), "image/png")
            .build();
        let (_, chapters, _) = load_epub_chapters(_f.path()).unwrap();
        render_chapter_blocks(&chapters, 0, 16.0)
    }

    #[golden_test(600, 200)]
    fn epub_image_data_url() -> cosmic::Element<'_, super::EpubViewerMessage> {
        let _guard = crate::test_support::cosmic_tk_lock();
        // Image embedded as a data URL — no EPUB resource entry needed.
        // Uses the same 8×8 orange PNG as epub_image, base64-encoded inline.
        const TINY_PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAIAAABLbSncAAAAEUlEQVR4nGM4kWKEFTEMLQkAVkZXgTy9n4kAAAAASUVORK5CYII=";
        let src = format!("data:image/png;base64,{TINY_PNG_B64}");
        let _f = EpubBuilder::new("Image Data URL")
            .body(format!(
                "<p>An image from a data URL follows:</p>\
                 <img src=\"{src}\" alt=\"Data URL image\"/>"
            ))
            .build();
        let (_, chapters, _) = load_epub_chapters(_f.path()).unwrap();
        render_chapter_blocks(&chapters, 0, 16.0)
    }

    #[golden_test(600, 150)]
    fn epub_image_missing_alt() -> cosmic::Element<'_, super::EpubViewerMessage> {
        let _guard = crate::test_support::cosmic_tk_lock();
        // Image whose src cannot be resolved — falls back to alt text.
        let _f = EpubBuilder::new("Image Alt")
            .body("<img src=\"images/missing.png\" alt=\"Missing image\"/>")
            .build();
        let (_, chapters, _) = load_epub_chapters(_f.path()).unwrap();
        render_chapter_blocks(&chapters, 0, 16.0)
    }

    #[golden_test(600, 300)]
    fn epub_svg() -> cosmic::Element<'_, super::EpubViewerMessage> {
        let _guard = crate::test_support::cosmic_tk_lock();
        let _f = EpubBuilder::new("SVG")
            .body(
                "<p>An inline SVG:</p>\
                 <svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 200 100\">\
                   <rect width=\"200\" height=\"100\" rx=\"10\" fill=\"#6060c0\"/>\
                   <circle cx=\"100\" cy=\"50\" r=\"30\" fill=\"#ffffff\" opacity=\"0.6\"/>\
                   <line x1=\"70\" y1=\"50\" x2=\"130\" y2=\"50\" stroke=\"#6060c0\" stroke-width=\"4\"/>\
                 </svg>",
            )
            .build();
        let (_, chapters, _) = load_epub_chapters(_f.path()).unwrap();
        render_chapter_blocks(&chapters, 0, 16.0)
    }
}

/// Block heights the pagination measures must match the heights the blocks
/// are laid out with.
#[cfg(test)]
mod height_tests {
    use cosmic::iced::Length;
    use cosmic::iced::font;
    use epub::BlockStyle;
    use epub::ContentBlock;
    use epub::InlineStyle;
    use epub::TextSpan;
    use rstest::rstest;

    use super::load_epub_chapters;
    use super::measure::BlockMeasurer;
    use super::measure::block_height;
    use super::render::RenderContext;
    use super::test_helper::EpubBuilder;
    use crate::page::epub_viewer::BlockHighlight;

    const FAMILY: font::Family = font::Family::Serif;
    const BASE_FONT_SIZE: f32 = 14.0;

    fn paragraph(text: &str, spans: Vec<TextSpan>, style: BlockStyle) -> ContentBlock {
        ContentBlock::Paragraph {
            text: text.to_string(),
            spans,
            style,
        }
    }

    fn span(text: &str, bold: bool, italic: bool) -> TextSpan {
        TextSpan {
            text: text.to_string(),
            style: InlineStyle {
                bold,
                italic,
                ..InlineStyle::default()
            },
            link: None,
            color: None,
            font_size_em: None,
        }
    }

    /// How the tests draw blocks.
    fn render_context(
        image_handles: &std::collections::HashMap<usize, cosmic::widget::image::Handle>,
    ) -> RenderContext<'_> {
        RenderContext {
            font_size: BASE_FONT_SIZE,
            family: FAMILY,
            max_image_height: 400.0,
            image_handles,
        }
    }

    /// Height `block` is laid out with by the real renderer at `width`.
    fn rendered_height(block: &ContentBlock, width: f32) -> f32 {
        let handles = std::collections::HashMap::new();
        let element = render_context(&handles).render_block(block, BlockHighlight::None);
        let extents = crate::test_support::layout_extents(
            cosmic::widget::scrollable(cosmic::widget::container(element).width(width)).into(),
            width,
            100.0,
        );
        extents.scrollables[0].content.height
    }

    // ── Paragraph height is exactly the rendered height ───────────────────────

    const LONG_TEXT: &str = "This is a longer paragraph that contains enough text to wrap \
        across multiple lines when rendered at the content width. It exercises the \
        multi-line path, and keeps going so that narrow widths wrap many times over.";

    #[rstest]
    #[case::plain_single_line(paragraph("A short paragraph.", vec![], BlockStyle::default()))]
    #[case::plain_multiline(paragraph(LONG_TEXT, vec![], BlockStyle::default()))]
    #[case::styled(paragraph(
        LONG_TEXT,
        vec![
            span(&LONG_TEXT[..60], false, true),
            span(&LONG_TEXT[60..140], true, false),
            span(&LONG_TEXT[140..], false, false),
        ],
        BlockStyle::default(),
    ))]
    #[case::margins_and_size(paragraph(
        LONG_TEXT,
        vec![],
        BlockStyle {
            font_size_em: Some(1.3),
            margin_top_em: Some(1.0),
            margin_bottom_em: Some(0.5),
            ..BlockStyle::default()
        },
    ))]
    fn paragraph_height_matches_rendered_height(
        #[case] block: ContentBlock,
        #[values(240.0, 600.0)] width: f32,
    ) {
        let _guard = crate::test_support::cosmic_tk_lock();
        cosmic_golden::init();
        let handles = std::collections::HashMap::new();
        let measured = block_height(
            &mut BlockMeasurer::default(),
            &render_context(&handles),
            &block,
            width,
        );
        let actual = rendered_height(&block, width);
        assert!(
            (measured - actual).abs() < 0.5,
            "width {width}: measured {measured:.1}, rendered {actual:.1}"
        );
    }

    // ── Every block type measures as it is laid out in a page column ──────────

    /// A PNG of `width` × `height` pixels.
    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::RgbImage::from_pixel(width, height, image::Rgb([200, 120, 40]))
            .write_to(&mut bytes, image::ImageFormat::Png)
            .expect("encode png");
        bytes.into_inner()
    }

    /// The only chapter of a book with `body` and `resources`.
    fn chapter(body: &str, resources: &[(&str, Vec<u8>, &'static str)]) -> super::EpubChapter {
        let mut builder = EpubBuilder::new("Measure").body(body);
        for (path, data, media_type) in resources {
            builder = builder.resource(*path, data.clone(), media_type);
        }
        let file = builder.build();
        let (_, mut chapters, _) = load_epub_chapters(file.path()).expect("load chapters");
        chapters.remove(0)
    }

    #[rstest]
    #[case::plain_heading(
        "Heading",
        "<h2>A chapter heading that is long enough to wrap</h2>",
        vec![],
    )]
    #[case::styled_heading(
        "Heading",
        "<h2>A <em>styled</em> heading that is long enough to wrap</h2>",
        vec![],
    )]
    #[case::unordered_list(
        "UnorderedList",
        "<ul><li>First item</li><li>A second item whose text is long enough to wrap \
         onto another line at narrow widths</li><li>Third with <strong>bold</strong> text</li></ul>",
        vec![],
    )]
    #[case::ordered_list(
        "OrderedList",
        "<ol><li>First step</li><li>A second step whose text is long enough to wrap \
         onto another line at narrow widths</li><li>Third with <em>italic</em> text</li></ol>",
        vec![],
    )]
    #[case::block_quote(
        "BlockQuote",
        "<blockquote><p>To be, or not to be, that is the question: whether 'tis nobler \
         in the mind to suffer.</p><p>William Shakespeare</p></blockquote>",
        vec![],
    )]
    #[case::plain_preformatted(
        "Preformatted",
        "<pre>fn hello() {\n    println!(\"Hello, world!\");\n}</pre>",
        vec![],
    )]
    #[case::styled_preformatted(
        "Preformatted",
        "<pre><b>fn</b> hello() {\n    <i>println!</i>(\"Hello, world!\");\n}</pre>",
        vec![],
    )]
    #[case::table(
        "Table",
        "<table><tr><th>Name</th><th>Age</th><th>City</th></tr>\
         <tr><td>Alice</td><td>30</td><td>Amsterdam</td></tr>\
         <tr><td>Bob</td><td>25</td><td>Berlin, with a remark long enough to wrap</td></tr>\
         </table>",
        vec![],
    )]
    #[case::horizontal_rule("HorizontalRule", "<p>Before.</p><hr/><p>After.</p>", vec![])]
    #[case::raster_image(
        "Image",
        "<p>A tall image:</p><img src=\"images/tall.png\" alt=\"Tall\"/>",
        vec![("OEBPS/images/tall.png", png(300, 600), "image/png")],
    )]
    #[case::svg(
        "Svg",
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 200 100\">\
         <rect width=\"200\" height=\"100\" fill=\"#6060c0\"/></svg>",
        vec![],
    )]
    #[case::footnote(
        "Footnote",
        "<p>Main text<sup><a href=\"#fn1\">1</a></sup>.</p>\
         <aside epub:type=\"footnote\" id=\"fn1\"><p>The footnote content explaining the \
         reference at some length.</p><p>A second footnote paragraph.</p></aside>",
        vec![],
    )]
    #[case::figure(
        "Figure",
        "<figure><svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 60\">\
         <rect width=\"100\" height=\"60\" fill=\"#4080c0\"/></svg>\
         <figcaption>Figure 1: A simple coloured rectangle.</figcaption></figure>",
        vec![],
    )]
    #[case::anchor("Anchor", "<p id=\"start\">Text after an anchor.</p>", vec![])]
    fn block_height_matches_layout_in_a_page_column(
        #[case] kind: &str,
        #[case] body: &str,
        #[case] resources: Vec<(&str, Vec<u8>, &'static str)>,
        #[values(240.0, 600.0)] width: f32,
    ) {
        let chapter = chapter(body, &resources);
        let _guard = crate::test_support::cosmic_tk_lock();
        cosmic_golden::init();
        assert!(
            chapter
                .blocks
                .iter()
                .any(|block| format!("{block:?}").starts_with(kind)),
            "fixture has no {kind} block: {:?}",
            chapter.blocks
        );
        let ctx = render_context(&chapter.image_handles);
        let spacing = cosmic::theme::active().cosmic().spacing.space_xxs as f32;

        let mut measurer = BlockMeasurer::default();
        let heights: Vec<f32> = chapter
            .blocks
            .iter()
            .map(|block| block_height(&mut measurer, &ctx, block, width))
            .collect();
        // Blocks measuring 0 are left out of the column, spacing included.
        let shown = heights.iter().filter(|&&height| height > 0.0).count();
        let measured = heights.iter().sum::<f32>() + spacing * shown.saturating_sub(1) as f32;

        let column_id = cosmic::widget::Id::new("measured-column");
        let column = chapter.blocks.iter().fold(
            cosmic::widget::Column::new()
                .spacing(spacing)
                .width(Length::Fill),
            |column, block| column.push(ctx.render_block(block, BlockHighlight::None)),
        );
        let extents = crate::test_support::layout_extents(
            cosmic::widget::scrollable(
                cosmic::widget::container(column)
                    .id(column_id.clone())
                    .width(width),
            )
            .into(),
            width,
            100.0,
        );
        let laid_out = extents
            .containers
            .iter()
            .find(|(id, _)| *id == column_id)
            .expect("column laid out")
            .1
            .height;
        assert!(
            (measured - laid_out).abs() < 0.5,
            "{kind} at width {width}: measured {measured:.1}, laid out {laid_out:.1}"
        );
    }
}

#[cfg(test)]
mod split_tests {
    use assert4rs::Assert;
    use cosmic::iced::font;
    use epub::BlockStyle;
    use epub::ContentBlock;
    use rstest::rstest;

    use super::BlockCut;
    use super::BlockMeasurer;
    use super::RenderContext;
    use super::try_split_block;

    const WIDTH: f32 = 600.0;
    const FONT_SIZE: f32 = 14.0;
    /// One line of body text at FONT_SIZE.
    const ONE_LINE_HEIGHT: f32 = FONT_SIZE * 1.4;

    fn split(text: &str, width: f32, available_height: f32) -> Option<usize> {
        let block = ContentBlock::Paragraph {
            text: text.to_string(),
            spans: vec![],
            style: BlockStyle::default(),
        };
        let image_handles = std::collections::HashMap::new();
        let ctx = RenderContext {
            font_size: FONT_SIZE,
            family: font::Family::Serif,
            max_image_height: 400.0,
            image_handles: &image_handles,
        };
        try_split_block(
            &ctx,
            &mut BlockMeasurer::default(),
            &block,
            width,
            available_height,
            BlockCut::default(),
        )
        .map(|cut| cut.char_offset)
    }

    // ── Early-exit conditions ────────────────────────────────────────────────

    #[test]
    fn empty_text_returns_none() {
        Assert::that(split("", WIDTH, 100.0)).is(None);
    }

    #[test]
    fn zero_height_returns_none() {
        Assert::that(split("some text", WIDTH, 0.0)).is(None);
    }

    #[test]
    fn negative_height_returns_none() {
        Assert::that(split("some text", WIDTH, -1.0)).is(None);
    }

    #[test]
    fn no_lines_fit_returns_none() {
        Assert::that(split("Short text.", WIDTH, 1.0)).is(None);
    }

    #[test]
    fn all_lines_fit_returns_none() {
        Assert::that(split("Short text.", WIDTH, 10_000.0)).is(None);
    }

    // ── Basic split contract ─────────────────────────────────────────────────

    #[rstest]
    #[case(ONE_LINE_HEIGHT)]
    #[case(ONE_LINE_HEIGHT * 2.0)]
    #[case(ONE_LINE_HEIGHT * 3.0)]
    fn split_offset_is_in_bounds(#[case] height: f32) {
        let text = "alpha bravo charlie delta echo foxtrot golf hotel india juliet kilo \
                    lima mike november oscar papa quebec romeo sierra tango uniform victor";
        if let Some(offset) = split(text, 200.0, height) {
            let char_count = text.chars().count();
            assert!(offset > 0, "split offset must be > 0, got {offset}");
            assert!(
                offset < char_count,
                "split offset {offset} must be < char count {char_count}"
            );
        }
    }

    // ── Multibyte / char-boundary safety (regression for the crash) ──────────

    /// Curly-quote characters (\u{201C}/\u{201D}) are 3-byte UTF-8 sequences;
    /// the split must not panic around them.
    #[test]
    fn multibyte_curly_quotes_dont_panic() {
        let text = "guards.\n Guards specify conditions that a request must satisfy in \
                    order to \u{201C}match\u{201D} and be passed over to the handler. \
                    From an implementation standpoint guards are implementors of the \
                    Guard trait: Guard::check is where the magic happens.";
        let _ = split(text, WIDTH, ONE_LINE_HEIGHT * 1.5);
    }

    /// Any `Some(offset)` returned must correspond to a valid UTF-8 char boundary
    /// when converted back to a byte index.
    #[test]
    fn split_result_lands_on_char_boundary() {
        let text = "guards. Guards specify conditions that a request must satisfy in \
                    order to \u{201C}match\u{201D} and be passed over to the handler. \
                    From an implementation standpoint guards are implementors of the \
                    Guard trait: Guard::check is where the magic happens.";
        for available_height in [
            ONE_LINE_HEIGHT,
            ONE_LINE_HEIGHT * 2.0,
            ONE_LINE_HEIGHT * 3.0,
        ] {
            if let Some(offset) = split(text, 200.0, available_height) {
                let byte_pos = text
                    .char_indices()
                    .nth(offset)
                    .map(|(i, _)| i)
                    .unwrap_or(text.len());
                assert!(
                    text.is_char_boundary(byte_pos),
                    "height={available_height}: byte offset {byte_pos} is not a char boundary"
                );
            }
        }
    }

    /// Text after a line break in the paragraph is counted, so a split after
    /// it lands at the start of a line.
    #[test]
    fn split_after_a_line_break_starts_the_next_line() {
        let text = "first line\nsecond line\nthird line";
        let offset = split(text, WIDTH, ONE_LINE_HEIGHT * 2.0).expect("two lines fit");
        let rest: String = text.chars().skip(offset).collect();
        Assert::that(rest.as_str()).is("third line");
    }

    // ── Word boundaries ──────────────────────────────────────────────────────

    /// A split lands at the start of a wrapped line, so the page's part of the
    /// paragraph ends with the whitespace the line wrapped at.
    #[test]
    fn split_does_not_break_inside_word() {
        let text = "alpha bravo charlie delta echo foxtrot golf hotel india juliet";
        let offset = split(text, 200.0, ONE_LINE_HEIGHT).expect("one line fits");
        let prefix: String = text.chars().take(offset).collect();
        assert!(
            prefix.ends_with(char::is_whitespace),
            "split prefix should end with whitespace, got: {prefix:?}"
        );
    }

    /// Word wrapping keeps a single very long word on one line, so there is
    /// nothing to split.
    #[test]
    fn single_long_word_is_not_split() {
        let text: String = "a".repeat(200);
        Assert::that(split(&text, WIDTH, ONE_LINE_HEIGHT)).is(None);
    }

    /// A code block part ending at a line boundary leaves out the whole line
    /// ending, so it draws no empty last line.
    #[rstest]
    #[case::newline("ab\ncd", 0, 3, 2)]
    #[case::carriage_return_newline("ab\r\ncd", 0, 4, 2)]
    #[case::carriage_return("ab\rcd", 0, 3, 2)]
    #[case::inside_a_line("abc\ncd", 0, 2, 2)]
    #[case::at_the_end("ab\n", 0, 3, 3)]
    #[case::continued_part("ab\ncd\nef", 3, 6, 5)]
    #[case::empty_part("ab\ncd", 3, 3, 3)]
    fn preformatted_part_leaves_out_its_last_line_ending(
        #[case] text: &str,
        #[case] start: usize,
        #[case] end: usize,
        #[case] expected: usize,
    ) {
        Assert::that(super::preformatted_part_end(text, start, end)).is(expected);
    }
}
