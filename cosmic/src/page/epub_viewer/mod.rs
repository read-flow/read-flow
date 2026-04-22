mod render;
#[cfg(test)]
mod test_helper;

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
use cosmic_text::FontSystem;
use epub::ContentBlock;
use epub::Document as EpubDocumentTrait;
use epub::EpubDocument;
use epub::NavEntry;
use epub::StyleSheet;
use epub::TextSpan;
use read_flow_core::Builder;

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
use crate::page::epub_viewer::render::RenderContext;
use crate::page::epub_viewer::render::render_partial_paragraph;
use crate::page::epub_viewer::render::render_partial_preformatted;
use crate::page::image_viewer::ViewerImage;

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
/// Minimum total viewer width below which the chapter sidebar pane is hidden.
const MIN_WIDTH_WITH_SIDEBAR: f32 = 600.0;

// --- Persistent reader preferences ---

/// Config version for EPUB reader preferences (individual key access).
const EPUB_PREFS_VERSION: u64 = 1;

/// Config key for the saved font family.
const KEY_FONT_FAMILY: &str = "epub_font_family";

/// Config key for the saved base font size (stored as integer pixels).
const KEY_BASE_FONT_SIZE: &str = "epub_base_font_size";

/// Config key for the saved view mode ("scroll" or "paginated").
const KEY_VIEW_MODE: &str = "epub_view_mode";

/// Config key for sidebar visibility (bool).
const KEY_SHOW_SIDEBAR: &str = "epub_show_sidebar";

/// Config key for dual-page mode ("auto", "off", "on").
const KEY_DUAL_PAGE: &str = "epub_dual_page";

/// Config key for the page margin in pixels (stored as u32, 0–128).
const KEY_PAGE_MARGIN: &str = "epub_page_margin";

/// All EPUB reader preferences stored in cosmic-config.
struct EpubPrefs {
    font_family: FontFamily,
    base_font_size: f32,
    view_mode: ViewMode,
    show_sidebar: bool,
    dual_page: DualPageMode,
    page_margin: f32,
}

impl Default for EpubPrefs {
    fn default() -> Self {
        EpubPrefs {
            font_family: FontFamily::default(),
            base_font_size: 16.0,
            view_mode: ViewMode::default(),
            show_sidebar: true,
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

    if let Ok(show) = ctx.get::<bool>(KEY_SHOW_SIDEBAR) {
        prefs.show_sidebar = show;
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

/// Save all EPUB reader preferences to cosmic-config.
fn save_epub_prefs(prefs: &EpubPrefs) {
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
    let _ = ctx.set(KEY_SHOW_SIDEBAR, prefs.show_sidebar);
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

/// A page is a contiguous range of blocks from the chapter's block list.
/// Stored as \[start .. end) (exclusive end).
///
/// When `start_char_offset > 0`, the first block (`blocks[start]`) is shown
/// starting from that character offset (continuation from the previous page).
/// When `end_char_offset > 0`, the last block (`blocks[end - 1]`) is shown only
/// up to that character offset (paragraph continues on the next page).
#[derive(Clone, Debug)]
struct PageRange {
    start: usize,
    /// First character of `blocks[start]` shown on this page (0 = from beginning).
    start_char_offset: usize,
    end: usize,
    /// Last character (exclusive) of `blocks[end - 1]` shown on this page (0 = to end).
    end_char_offset: usize,
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

#[derive(Debug)]
pub enum EpubViewerOutput {
    /// Carries the fingerprint and the opaque progress JSON to persist, if any.
    Close(Fingerprint, Option<String>),
    /// Open the image viewer page for the given image.
    OpenImageViewer(ViewerImage),
    NavSubEntriesChanged,
    NavSubEntryActivated(usize),
}

impl Clone for EpubViewerOutput {
    fn clone(&self) -> Self {
        match self {
            EpubViewerOutput::Close(fp, progress) => {
                EpubViewerOutput::Close(fp.clone(), progress.clone())
            }
            EpubViewerOutput::OpenImageViewer(img) => {
                EpubViewerOutput::OpenImageViewer(img.clone())
            }
            EpubViewerOutput::NavSubEntriesChanged => EpubViewerOutput::NavSubEntriesChanged,
            EpubViewerOutput::NavSubEntryActivated(idx) => {
                EpubViewerOutput::NavSubEntryActivated(*idx)
            }
        }
    }
}

#[derive(Debug, Clone)]
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
    /// Toggle chapter navigation sidebar visibility.
    ShowSidebar(bool),
    /// Toggle the nav dropdown (narrow-window alternative to the sidebar).
    ToggleNavDropdown,
    /// Set the font family for rendering (index into FontFamily::ALL).
    SetFontFamily(FontFamily),
    /// Set the base body font size in pixels (12–24).
    SetBaseFontSize(f32),
    /// The EPUB file could not be opened; carries the error message.
    LoadFailed(String),
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
    /// Copy a code block's plain text to the clipboard.
    CopyCodeBlock(String),
    /// Open the image viewer page for the given image.
    OpenImageViewer(ViewerImage),
}

// --- EpubViewer page ---

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
    /// Most recently observed viewport dimensions, set from the `responsive`
    /// closure (via Cell, since `view()` takes `&self`).
    viewport_size: Cell<(f32, f32)>,
    /// Whether to render two pages side by side in paginated mode.
    dual_page: DualPageMode,
    /// Deferred block index for page restoration.  Set when reading progress
    /// is loaded but pagination hasn't run yet (viewport_size still 0×0).
    /// Consumed by `maybe_repaginate()` once a valid layout is available.
    pending_block_index: Option<usize>,
    /// Whether the chapter navigation sidebar is visible.
    show_sidebar: bool,
    /// Whether the sidebar pane was visible in the last rendered frame.
    /// Used for hysteresis when auto-hiding on narrow windows.
    sidebar_pane_visible: Cell<bool>,
    /// Whether the nav dropdown (shown on narrow windows instead of the sidebar) is open.
    nav_dropdown_open: bool,
    /// Font family used for rendering EPUB content.
    font_family: FontFamily,
    /// Pre-computed display names for the font family dropdown.
    font_family_names: widget::combo_box::State<FontFamily>,
    /// Ordered nav entries from the EPUB TOC (with depth and fragment-preserving hrefs).
    nav_entries: Vec<NavEntry>,
    /// Index of the most recently activated nav entry, for precise sidebar highlighting.
    active_nav_entry: Option<usize>,
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
    /// Page margin in pixels applied to all four sides of the page content area (0–128).
    page_margin: f32,
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
            viewport_size: Cell::new((0.0, 0.0)),
            dual_page: saved_prefs.dual_page,
            pending_block_index: None,
            show_sidebar: saved_prefs.show_sidebar,
            sidebar_pane_visible: Cell::new(true),
            nav_dropdown_open: false,
            font_family: saved_prefs.font_family,
            font_family_names: widget::combo_box::State::new(FontFamily::all()),
            nav_entries: Vec::new(),
            active_nav_entry: None,
            base_font_size: saved_prefs.base_font_size,
            highlighted_block: None,
            search_visible: false,
            search_query: String::new(),
            search_matches: Vec::new(),
            search_current: 0,
            search_input_id: widget::Id::unique(),
            font_system: RefCell::new(FontSystem::new()),
            page_margin: saved_prefs.page_margin,
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

    /// Snapshot the current viewer preferences and persist them to cosmic-config.
    fn save_current_prefs(&self) {
        save_epub_prefs(&EpubPrefs {
            font_family: self.font_family,
            base_font_size: self.base_font_size,
            view_mode: self.view_mode,
            show_sidebar: self.show_sidebar,
            dual_page: self.dual_page,
            page_margin: self.page_margin,
        });
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
        let cosmic_theme::Spacing { space_s, .. } = theme::active().cosmic().spacing;

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
        let mut column = widget::column::with_capacity(capacity);

        if self.nav_entries.is_empty() {
            for (idx, chapter) in self.chapters.iter().enumerate() {
                let active = idx == self.active_chapter;
                let mut label = widget::text::body(&chapter.label)
                    .wrapping(cosmic::iced::widget::text::Wrapping::None);
                if active {
                    label = label.font(font::Font {
                        weight: font::Weight::Bold,
                        ..Default::default()
                    });
                }
                let button = widget::button::custom(label)
                    .class(widget::button::ButtonClass::Link)
                    .on_press(EpubViewerMessage::SelectChapter(idx))
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
                let is_active_chapter = base == current_href;
                // For non-active chapters, only show the first nav entry of that
                // chapter (the "group leader") and collapse the rest.
                let is_group_leader = !self.nav_entries[..idx]
                    .iter()
                    .any(|e| e.href.split_once('#').map(|(b, _)| b).unwrap_or(&e.href) == base);
                if !is_active_chapter && !is_group_leader {
                    continue;
                }
                let active = self.active_nav_entry == Some(idx);
                let mut label = widget::text::body(&entry.label)
                    .wrapping(cosmic::iced::widget::text::Wrapping::None);
                if active {
                    label = label.font(font::Font {
                        weight: font::Weight::Bold,
                        ..Default::default()
                    });
                }
                let button = widget::button::custom(label)
                    .class(widget::button::ButtonClass::Link)
                    .on_press(EpubViewerMessage::SelectNavEntry(idx))
                    .width(Length::Fill);
                let indent = (entry.depth as f32) * (space_s as f32);
                let row = widget::Row::new()
                    .push(widget::Space::new().width(Length::Fixed(indent)))
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
            widget::scrollable(column)
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
        ])
        .width(Length::Fixed(CHAPTER_SIDEBAR_WIDTH))
        .height(Length::Fill)
        .into()
    }

    /// A full-width collapsible nav menu shown above the document content on
    /// narrow windows where the sidebar pane does not fit.
    fn view_nav_dropdown(&self) -> Element<'_, EpubViewerMessage> {
        let cosmic_theme::Spacing {
            space_xxs, space_s, ..
        } = theme::active().cosmic().spacing;

        let current_label: &str = if let Some(idx) = self.active_nav_entry {
            self.nav_entries
                .get(idx)
                .map(|e| e.label.as_str())
                .unwrap_or("")
        } else {
            self.chapters
                .get(self.active_chapter)
                .map(|c| c.label.as_str())
                .unwrap_or("")
        };

        let chapter_info = if !self.chapters.is_empty() {
            format!("{} / {}", self.active_chapter + 1, self.chapters.len())
        } else {
            String::new()
        };

        let chevron = if self.nav_dropdown_open {
            "go-up-symbolic"
        } else {
            "go-down-symbolic"
        };

        let header_content = widget::Row::new()
            .push(
                widget::text::body(current_label)
                    .wrapping(cosmic::iced::widget::text::Wrapping::None)
                    .width(Length::Fill),
            )
            .push(widget::text::body(chapter_info))
            .push(widget::icon::from_name(chevron).size(ICON_SIZE).icon())
            .align_y(Vertical::Center)
            .spacing(space_s)
            .padding([space_xxs, space_s]);

        let header_btn = widget::button::custom(header_content)
            .class(widget::button::ButtonClass::ListItem)
            .on_press(EpubViewerMessage::ToggleNavDropdown)
            .width(Length::Fill);

        let mut col = widget::Column::new().push(header_btn);

        if self.nav_dropdown_open {
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
            let mut entries_col = widget::column::with_capacity(capacity);

            if self.nav_entries.is_empty() {
                for (idx, chapter) in self.chapters.iter().enumerate() {
                    let active = idx == self.active_chapter;
                    let mut label = widget::text::body(&chapter.label)
                        .wrapping(cosmic::iced::widget::text::Wrapping::None);
                    if active {
                        label = label.font(font::Font {
                            weight: font::Weight::Bold,
                            ..Default::default()
                        });
                    }
                    let button = widget::button::custom(label)
                        .class(widget::button::ButtonClass::Link)
                        .on_press(EpubViewerMessage::SelectChapter(idx))
                        .width(Length::Fill);
                    entries_col = entries_col.push(button);
                }
            } else {
                for (idx, entry) in self.nav_entries.iter().enumerate() {
                    let base = entry
                        .href
                        .split_once('#')
                        .map(|(b, _)| b)
                        .unwrap_or(&entry.href);
                    let is_active_chapter = base == current_href;
                    let is_group_leader = !self.nav_entries[..idx]
                        .iter()
                        .any(|e| e.href.split_once('#').map(|(b, _)| b).unwrap_or(&e.href) == base);
                    if !is_active_chapter && !is_group_leader {
                        continue;
                    }
                    let active = self.active_nav_entry == Some(idx);
                    let mut label = widget::text::body(&entry.label)
                        .wrapping(cosmic::iced::widget::text::Wrapping::None);
                    if active {
                        label = label.font(font::Font {
                            weight: font::Weight::Bold,
                            ..Default::default()
                        });
                    }
                    let button = widget::button::custom(label)
                        .class(widget::button::ButtonClass::Link)
                        .on_press(EpubViewerMessage::SelectNavEntry(idx))
                        .width(Length::Fill);
                    let indent = (entry.depth as f32) * (space_s as f32);
                    let row = widget::Row::new()
                        .push(widget::Space::new().width(Length::Fixed(indent)))
                        .push(button);
                    entries_col = entries_col.push(row);
                }
            }

            col = col.push(
                widget::container(
                    widget::scrollable(entries_col)
                        .height(Length::Fixed(280.0))
                        .width(Length::Fill),
                )
                .class(Container::Secondary)
                .width(Length::Fill),
            );
        }

        widget::container(col)
            .class(Container::Secondary)
            .width(Length::Fill)
            .into()
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
                    widget::text_editor(&self.raw_html_content)
                        .on_action(EpubViewerMessage::RawHtmlAction)
                        .font(cosmic::font::mono())
                        .apply(widget::container)
                        .width(Length::Fill),
                );
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
                    column = column.push(
                        RenderContext {
                            font_size: self.base_font_size,
                            family,
                            max_image_height: f32::MAX,
                            image_handles: &chapter.image_handles,
                        }
                        .render_block(block, highlight),
                    );
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
        let cosmic_theme::Spacing { space_xxs, .. } = theme::active().cosmic().spacing;

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

        let max_content_width = 800.0_f32;
        let base_font_size = self.base_font_size;
        let page_margin = self.page_margin;

        widget::responsive(move |size| {
            // Store viewport size so update() can trigger re-pagination.
            self.viewport_size.set((size.width, size.height));

            // In raw HTML mode, fall back to a simple scrollable view.
            if show_raw_html {
                return widget::container(
                    widget::text_editor(&self.raw_html_content)
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
                    let cw = (pw - page_margin * 2.0).max(1.0);
                    let ch = (size.height - page_margin * 2.0 - sp_s * 2.0 - 24.0).max(1.0);
                    computed_layout = paginate_blocks(
                        &chapter.blocks,
                        ch,
                        cw,
                        base_font_size,
                        sp_xxs,
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

                let mut column = widget::Column::new().spacing(space_xxs).width(Length::Fill);
                if let Some(range) = page_range {
                    let block_count = range.end - range.start;
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
                        // For split paragraphs, render only the visible char range using
                        // owned data so the element lifetime does not depend on locals.
                        let start_off = if i == 0 { range.start_char_offset } else { 0 };
                        let end_off = if i + 1 == block_count && range.end_char_offset > 0 {
                            range.end_char_offset
                        } else {
                            usize::MAX
                        };
                        let el = if start_off > 0 || end_off < usize::MAX {
                            if let ContentBlock::Paragraph { text, spans, style } = block {
                                let char_count = text.chars().count();
                                let s = start_off.min(char_count);
                                let e = end_off.min(char_count);
                                render_partial_paragraph(
                                    text[char_offset_to_byte(text, s)
                                        ..char_offset_to_byte(text, e)]
                                        .to_string(),
                                    slice_spans(spans, s, e),
                                    style,
                                    highlight,
                                    base_font_size,
                                    family,
                                )
                            } else if let ContentBlock::Preformatted { text, spans, .. } = block {
                                let char_count = text.chars().count();
                                let s = start_off.min(char_count);
                                let e = end_off.min(char_count);
                                let full_text_to_copy = (start_off == 0).then(|| text.clone());
                                render_partial_preformatted(
                                    text[char_offset_to_byte(text, s)
                                        ..char_offset_to_byte(text, e)]
                                        .to_string(),
                                    slice_spans(spans, s, e),
                                    base_font_size,
                                    full_text_to_copy,
                                )
                            } else {
                                RenderContext {
                                    font_size: base_font_size,
                                    family,
                                    max_image_height: layout.page_height * 0.9,
                                    image_handles: &chapter.image_handles,
                                }
                                .render_block(block, highlight)
                            }
                        } else {
                            RenderContext {
                                font_size: base_font_size,
                                family,
                                max_image_height: layout.page_height * 0.9,
                                image_handles: &chapter.image_handles,
                            }
                            .render_block(block, highlight)
                        };
                        column = column.push(el);
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
                let paper_content = widget::Column::new()
                    .push(
                        widget::scrollable(
                            widget::container(
                                widget::container(column)
                                    .padding(page_margin)
                                    .max_width(max_content_width)
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

            // Build the center content: single or dual page.
            let center_content: Element<'_, EpubViewerMessage> = if dual {
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
            };

            // Outer "desk" with click-to-turn zones.
            let left_zone = widget::mouse_area(
                widget::container(widget::icon::from_name("go-previous-symbolic").size(ICON_SIZE))
                    .width(Length::FillPortion(1))
                    .height(Length::Fill)
                    .center(Length::Fill),
            )
            .on_press(EpubViewerMessage::PreviousPage);

            let center = widget::container(center_content)
                .width(Length::FillPortion(10))
                .height(Length::Fill);

            let right_zone = widget::mouse_area(
                widget::container(widget::icon::from_name("go-next-symbolic").size(ICON_SIZE))
                    .width(Length::FillPortion(1))
                    .height(Length::Fill)
                    .center(Length::Fill),
            )
            .on_press(EpubViewerMessage::NextPage);

            // Cap the reading area so wide screens don't produce huge empty desk
            // margins. The click zones are FillPortion(1) each and the center is
            // FillPortion(8), so the center takes 80% of the row. We size the cap
            // to just fit the page(s) plus proportional click zones.
            let center_pages_width = if dual {
                max_content_width * 2.0 + space_xxs as f32
            } else {
                max_content_width
            };
            let max_row_width = center_pages_width / 0.8;

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
                .style(desk_background)
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

        // Use the content width recorded in the last layout pass (set inside
        // view_content's responsive closure) to decide whether to show the
        // sidebar pane. Hysteresis prevents oscillation when the window
        // width straddles the threshold.
        let (content_width, _) = self.viewport_size.get();
        let was_visible = self.sidebar_pane_visible.get();
        let show_sidebar_now = if !self.show_sidebar {
            false
        } else if content_width == 0.0 {
            // First frame: default to showing sidebar
            true
        } else if was_visible {
            // Pane is shown: total ≈ content + sidebar; hide only if total < threshold
            content_width + CHAPTER_SIDEBAR_WIDTH >= MIN_WIDTH_WITH_SIDEBAR
        } else {
            // Pane is hidden: total ≈ content; show only if content >= threshold
            content_width >= MIN_WIDTH_WITH_SIDEBAR
        };
        self.sidebar_pane_visible.set(show_sidebar_now);

        // Show dropdown nav above the content when the sidebar doesn't fit.
        let show_nav_dropdown = self.show_sidebar && !show_sidebar_now;

        let main_col = widget::Column::new()
            .height(Length::Fill)
            .push_maybe(show_nav_dropdown.then(|| self.view_nav_dropdown()))
            .push_maybe(self.search_visible.then(|| self.view_search_bar()))
            .push(self.view_content());

        widget::Row::new()
            .height(Length::Fill)
            .push_maybe(show_sidebar_now.then(|| self.view_chapter_sidebar()))
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

    fn view_header_start(&self) -> Vec<Element<'_, EpubViewerMessage>> {
        let cosmic_theme::Spacing { space_xxs, .. } = theme::active().cosmic().spacing;

        vec![
            widget::button::icon(widget::icon::from_name("go-previous-symbolic").size(ICON_SIZE))
                .on_press(EpubViewerMessage::Out(EpubViewerOutput::Close(
                    self.fingerprint.clone(),
                    if self.chapters.is_empty() {
                        None
                    } else {
                        // In paginated mode use the first block of the current page.
                        // In scroll mode derive the block index from scroll_y so the
                        // position can be accurately restored on any device regardless
                        // of display dimensions.
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
                        Some(serialize_progress(
                            self.active_chapter,
                            self.scroll_y,
                            first_block,
                            self.view_mode,
                            self.base_font_size,
                        ))
                    },
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
                    .step(4.0),
                ),
            );
        }

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

    fn nav_sub_entries(&self) -> Vec<crate::page::NavSubEntry> {
        self.nav_entries
            .iter()
            .map(|entry| crate::page::NavSubEntry {
                label: entry.label.clone(),
                icon: None,
                indent: entry.depth as u16,
            })
            .collect()
    }

    fn active_nav_sub_entry(&self) -> Option<usize> {
        self.active_nav_entry
    }

    fn on_nav_sub_entry_selected(&mut self, index: usize) -> Task<Action<EpubViewerMessage>> {
        self.update(EpubViewerMessage::SelectNavEntry(index))
    }

    fn update(&mut self, message: EpubViewerMessage) -> Task<Action<EpubViewerMessage>> {
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
                if !self.chapters.is_empty() {
                    self.active_chapter = self
                        .initial_chapter
                        .unwrap_or(0)
                        .min(self.chapters.len() - 1);
                }
                self.sync_raw_html_content();
                // Restore scroll position once content is available.
                // In scroll mode, prefer block_index over raw scroll_y so
                // the position is layout-independent (accurate across devices
                // with different display dimensions or font size settings).
                let target_y = if self.view_mode == ViewMode::Scroll {
                    if let Some(bi) = self.pending_block_index.take() {
                        let content_w = 800.0;
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
                let scroll_task = if let Some(y) = target_y {
                    self.scroll_y = y;
                    scrollable::scroll_to(
                        self.content_scroll_id.clone(),
                        scrollable::AbsoluteOffset { x: 0.0, y }.into(),
                    )
                } else {
                    Task::none()
                };
                Task::batch([
                    scroll_task,
                    Task::done(cosmic::action::app(EpubViewerMessage::Out(
                        EpubViewerOutput::NavSubEntriesChanged,
                    ))),
                ])
            }
            EpubViewerMessage::ReadingProgressLoaded(pos) => {
                self.initial_chapter = pos.chapter;
                self.scroll_y = pos.scroll_y;
                if !self.chapters.is_empty()
                    && let Some(c) = pos.chapter
                    && c < self.chapters.len()
                {
                    self.active_chapter = c;
                    self.sync_raw_html_content();
                }
                // Restore view mode if it was persisted.
                if let Some(mode) = pos.view_mode {
                    self.view_mode = mode;
                }
                // Store block index for deferred restoration in both modes.
                // For paginated mode, `maybe_repaginate()` consumes it to
                // find the right page once a valid viewport is known.
                // For scroll mode, if chapters are already loaded we compute
                // the y offset immediately; otherwise EpubLoaded will do it.
                self.pending_block_index = pos.block_index;
                if self.view_mode == ViewMode::Scroll && !self.chapters.is_empty() {
                    // Chapters loaded before progress — compute y now.
                    let target_y = if let Some(bi) = self.pending_block_index.take() {
                        let content_w = 800.0;
                        let chapter_idx = self.active_chapter;
                        self.ensure_block_heights(chapter_idx, content_w);
                        self.block_heights_cache
                            .get(&chapter_idx)
                            .map(|(_, _, heights)| y_for_block_index_from_heights(heights, bi))
                    } else if self.scroll_y > 0.0 {
                        Some(self.scroll_y)
                    } else {
                        None
                    };
                    if let Some(y) = target_y {
                        self.scroll_y = y;
                        return scrollable::scroll_to(
                            self.content_scroll_id.clone(),
                            scrollable::AbsoluteOffset { x: 0.0, y }.into(),
                        );
                    }
                }
                Task::none()
            }
            EpubViewerMessage::SelectChapter(idx) => {
                self.nav_dropdown_open = false;
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
                self.nav_dropdown_open = false;
                self.active_nav_entry = Some(nav_idx);
                let nav_activated: Task<Action<EpubViewerMessage>> =
                    Task::done(cosmic::action::app(EpubViewerMessage::Out(
                        EpubViewerOutput::NavSubEntryActivated(nav_idx),
                    )));
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
                                    let content_w = 800.0;
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
                        let content_w = 800.0;
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
                            let mut blocks = epub::content::parse_xhtml(
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
            EpubViewerMessage::ShowSidebar(show) => {
                self.show_sidebar = show;
                self.save_current_prefs();
                Task::none()
            }
            EpubViewerMessage::ToggleNavDropdown => {
                self.nav_dropdown_open = !self.nav_dropdown_open;
                Task::none()
            }
            EpubViewerMessage::SetFontFamily(family) => {
                self.font_family = family;
                self.pagination_cache.clear();
                self.block_heights_cache.clear();
                self.maybe_repaginate();
                self.save_current_prefs();
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
            EpubViewerMessage::LoadFailed(error) => {
                tracing::warn!("EPUB load failed: {error}");
                self.load_error = Some(error);
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
fn serialize_progress(
    chapter: usize,
    scroll_y: f32,
    first_block: usize,
    view_mode: ViewMode,
    font_size: f32,
) -> String {
    let mode = match view_mode {
        ViewMode::Scroll => "scroll",
        ViewMode::Paginated => "paginated",
    };
    format!(
        "{{\"chapter\":{chapter},\"scroll\":{scroll_y},\"block\":{first_block},\
\"mode\":\"{mode}\",\"font_size\":{font_size}}}"
    )
}

/// Parsed reading position from a progress JSON string.
#[derive(Clone, Debug, Default)]
pub(crate) struct ReadingPosition {
    chapter: Option<usize>,
    scroll_y: f32,
    /// Index of the first visible block (for paginated and scroll mode restoration).
    block_index: Option<usize>,
    /// View mode that was active when progress was saved.
    view_mode: Option<ViewMode>,
    /// Base font size that was active when progress was saved (px).
    font_size: Option<f32>,
}

/// Parse reading progress from a JSON string like
/// `{"chapter":2,"scroll":340.5,"block":15,"mode":"paginated"}`.
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
                    "font_size" => pos.font_size = value.trim().parse().ok(),
                    _ => {}
                }
            }
        }
    }
    pos
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

        let (blocks, raw_html) = match epub_doc.resolve_resource(&item.href) {
            Ok(data) => {
                let href = &item.href;
                let raw = String::from_utf8_lossy(&data).into_owned();
                let stylesheet = load_chapter_stylesheets(&raw, href, &epub_doc);
                let mut blocks =
                    epub::content::parse_xhtml(&data, href, &stylesheet, &mut |img_path| {
                        match epub_doc.resolve_resource(img_path) {
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
                        }
                    });
                resolve_svg_blocks(&mut blocks, href, &epub_doc);
                (blocks, raw)
            }
            Err(e) => {
                tracing::warn!("failed to resolve spine item {}: {e}", item.href);
                (Vec::new(), String::new())
            }
        };

        let mut image_handles = HashMap::new();
        collect_image_handles(&blocks, &mut image_handles);
        chapters.push(EpubChapter {
            label,
            href: item.href.clone(),
            blocks,
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

// ── COSMIC typography constants ──────────────────────────────────────────────
// These match the exact values used by widget::text::title1/2/3/4/heading/body
// in libcosmic's widget/text.rs.  They are independent of the user's
// `base_font_size` setting: plain (unstyled) headings always render at these
// sizes, so the height estimator must use them too.
const COSMIC_TITLE1_SIZE: f32 = 35.0;
const COSMIC_TITLE1_LINE_HEIGHT: f32 = 52.0;
const COSMIC_TITLE2_SIZE: f32 = 29.0;
const COSMIC_TITLE2_LINE_HEIGHT: f32 = 43.0;
const COSMIC_TITLE3_SIZE: f32 = 24.0;
const COSMIC_TITLE3_LINE_HEIGHT: f32 = 36.0;
const COSMIC_TITLE4_SIZE: f32 = 20.0;
const COSMIC_TITLE4_LINE_HEIGHT: f32 = 30.0;
const COSMIC_HEADING_SIZE: f32 = 14.0;
const COSMIC_HEADING_LINE_HEIGHT: f32 = 21.0;
// Body text: widget::text::body() sets line_height=21 and font_size=14.
// The render calls .size(user_font_size) to override font size, but the
// line_height stays at 21.0, so we use that for unstyled paragraphs too.
const COSMIC_BODY_LINE_HEIGHT: f32 = 21.0;

/// Measure the pixel height that `text` occupies when shaped with explicit
/// `font_size` and `line_height` and wrapped to `content_width`.
/// This is the core measurement primitive used by the height estimator.
fn measure_text_height_with_line_height(
    text: &str,
    content_width: f32,
    font_size: f32,
    line_height: f32,
    font_system: &mut FontSystem,
) -> f32 {
    use cosmic_text::Attrs;
    use cosmic_text::Buffer;
    use cosmic_text::Metrics;
    use cosmic_text::Shaping;
    let metrics = Metrics::new(font_size, line_height);
    let mut buffer = Buffer::new(font_system, metrics);
    buffer.set_size(Some(content_width), None);
    buffer.set_text(text, &Attrs::new(), Shaping::Advanced, None);
    buffer.shape_until_scroll(font_system, false);
    buffer
        .layout_runs()
        .last()
        .map(|r| r.line_y + r.line_height)
        .unwrap_or(line_height)
}

/// Measure the pixel height that `text` occupies when shaped at `font_size` and wrapped to
/// `content_width`, using `cosmic-text` for accurate glyph metrics (Phase 4b).
/// Line height is derived as `font_size * 1.375` — use
/// [`measure_text_height_with_line_height`] when the line height differs.
fn measure_text_height(
    text: &str,
    content_width: f32,
    font_size: f32,
    font_system: &mut FontSystem,
) -> f32 {
    measure_text_height_with_line_height(
        text,
        content_width,
        font_size,
        font_size * 1.375,
        font_system,
    )
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

/// Shape `text` and return the character offset at which lines first exceed
/// `available_height` pixels.  Returns `None` when all lines fit or the text is empty.
fn find_split_char_offset(
    text: &str,
    content_width: f32,
    font_size: f32,
    available_height: f32,
    font_system: &mut FontSystem,
) -> Option<usize> {
    if text.is_empty() || available_height <= 0.0 {
        return None;
    }
    use cosmic_text::Attrs;
    use cosmic_text::Buffer;
    use cosmic_text::Metrics;
    use cosmic_text::Shaping;
    let line_height = font_size * 1.375;
    let metrics = Metrics::new(font_size, line_height);
    let mut buffer = Buffer::new(font_system, metrics);
    buffer.set_size(Some(content_width), None);
    buffer.set_text(text, &Attrs::new(), Shaping::Advanced, None);
    buffer.shape_until_scroll(font_system, false);
    let mut last_byte_end: Option<usize> = None;
    for run in buffer.layout_runs() {
        if run.line_y + run.line_height > available_height {
            break;
        }
        if let Some(g) = run.glyphs.last() {
            last_byte_end = Some(g.end);
        }
    }

    // cosmic-text glyph endpoints can land mid-codepoint for multi-byte
    // characters (e.g. curly quotes).  Walk back to the nearest valid
    // UTF-8 boundary before slicing.
    let mut b = last_byte_end?;
    while b > 0 && !text.is_char_boundary(b) {
        b -= 1;
    }
    if b == 0 {
        return None;
    }
    let prefix = &text[..b];

    // Cosmic-text wraps at word boundaries, but force-breaks long words at
    // character boundaries.  If the raw split point is mid-word, walk back to
    // the nearest preceding whitespace so we never split inside a word.
    if !prefix.ends_with(char::is_whitespace)
        && let Some(ws_byte) = prefix.rfind(char::is_whitespace)
    {
        // Put the whitespace on the current page; next page starts with
        // the first letter of the word that would have been split.
        let ws_char_len = text[ws_byte..].chars().next().map_or(1, |c| c.len_utf8());
        return Some(text[..ws_byte + ws_char_len].chars().count());
    }
    // No whitespace before the force-break (one very long word with no
    // preceding space in the suffix): fall through and return the raw
    // character offset — the caller will let the block overflow rather
    // than loop forever.

    Some(prefix.chars().count())
}

/// Height of `block` starting from `start_char_offset` characters in.
/// Falls back to the full block height when `start_char_offset == 0` or when the
/// block type does not support partial measurement.
fn effective_block_height(
    block: &ContentBlock,
    start_char_offset: usize,
    content_width: f32,
    font_size: f32,
    font_system: &mut FontSystem,
) -> f32 {
    if start_char_offset == 0 {
        return estimated_block_height_for_width(block, content_width, font_size, font_system);
    }
    match block {
        ContentBlock::Paragraph { text, .. } => {
            let suffix = &text[char_offset_to_byte(text, start_char_offset)..];
            measure_text_height(suffix, content_width, font_size, font_system)
                .max(font_size * 1.375)
        }
        ContentBlock::Heading { text, level, .. } => {
            let heading_size = match level {
                1 => font_size * 2.0,
                2 => font_size * 1.75,
                3 => font_size * 1.5,
                4 => font_size * 1.25,
                _ => font_size * 1.125,
            };
            let suffix = &text[char_offset_to_byte(text, start_char_offset)..];
            measure_text_height(suffix, content_width, heading_size, font_system)
                .max(heading_size * 1.375)
        }
        ContentBlock::Preformatted { text, .. } => {
            let line_h = font_size * 1.375;
            let byte_start = char_offset_to_byte(text, start_char_offset);
            let line_count = text[byte_start..].lines().count();
            (line_count as f32 * line_h).max(line_h)
        }
        _ => estimated_block_height_for_width(block, content_width, font_size, font_system),
    }
}

/// Attempt to split `block` so that text up to `available_height` pixels fits on the current
/// page.  `start_char_offset` is the offset already skipped (for continued splits).
/// Returns the absolute char offset where the split should occur, or `None` if unsplittable.
fn try_split_block(
    block: &ContentBlock,
    content_width: f32,
    font_size: f32,
    available_height: f32,
    start_char_offset: usize,
    font_system: &mut FontSystem,
) -> Option<usize> {
    match block {
        ContentBlock::Paragraph { text, .. } => {
            let byte_start = char_offset_to_byte(text, start_char_offset);
            let suffix = &text[byte_start..];
            find_split_char_offset(
                suffix,
                content_width,
                font_size,
                available_height,
                font_system,
            )
            .map(|rel| start_char_offset + rel)
        }
        ContentBlock::Preformatted { text, .. } => {
            let line_h = font_size * 1.375;
            let lines_that_fit = (available_height / line_h).floor() as usize;
            if lines_that_fit == 0 {
                return None;
            }
            let byte_start = char_offset_to_byte(text, start_char_offset);
            let suffix = &text[byte_start..];
            // Walk through lines until we have consumed `lines_that_fit` of them.
            // The split point is the char offset just after the final fitting newline.
            let mut lines_seen = 0usize;
            let mut split_byte: Option<usize> = None;
            for (byte_idx, ch) in suffix.char_indices() {
                if ch == '\n' {
                    lines_seen += 1;
                    if lines_seen == lines_that_fit {
                        split_byte = Some(byte_idx + 1); // start of next line
                        break;
                    }
                }
            }
            let byte_offset = split_byte?;
            // Convert to char offset relative to the full text.
            let rel_char_offset = suffix[..byte_offset].chars().count();
            Some(start_char_offset + rel_char_offset)
        }
        _ => None,
    }
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
        ContentBlock::Heading {
            level, text, spans, ..
        } => {
            if spans.is_empty() {
                // Plain headings render via widget::text::title1/2/3/4/heading which
                // use fixed COSMIC typography sizes independent of base_font_size.
                let (h_size, h_line_h) = match level {
                    1 => (COSMIC_TITLE1_SIZE, COSMIC_TITLE1_LINE_HEIGHT),
                    2 => (COSMIC_TITLE2_SIZE, COSMIC_TITLE2_LINE_HEIGHT),
                    3 => (COSMIC_TITLE3_SIZE, COSMIC_TITLE3_LINE_HEIGHT),
                    4 => (COSMIC_TITLE4_SIZE, COSMIC_TITLE4_LINE_HEIGHT),
                    _ => (COSMIC_HEADING_SIZE, COSMIC_HEADING_LINE_HEIGHT),
                };
                measure_text_height_with_line_height(
                    text,
                    content_width,
                    h_size,
                    h_line_h,
                    font_system,
                )
                .max(h_line_h)
            } else {
                // Styled headings use render_spans() which applies font_size * multiplier.
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
        }
        ContentBlock::Paragraph { text, spans, .. } => {
            if spans.is_empty() {
                // widget::text::body().size(font_size) keeps line_height=21 regardless
                // of font_size, so we measure with the fixed COSMIC body line height.
                measure_text_height_with_line_height(
                    text,
                    content_width,
                    font_size,
                    COSMIC_BODY_LINE_HEIGHT.max(font_size * 1.375),
                    font_system,
                )
                .max(COSMIC_BODY_LINE_HEIGHT.max(line_h))
            } else {
                measure_text_height(text, content_width, font_size, font_system).max(line_h)
            }
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
/// Paragraphs that straddle a page boundary are split at a word-boundary
/// character offset computed via cosmic-text layout runs (Phase 4c).
/// Other block types that exceed `page_height` get their own page unchanged.
fn paginate_blocks(
    blocks: &[ContentBlock],
    page_height: f32,
    page_width: f32,
    font_size: f32,
    block_spacing: f32,
    font_system: &mut FontSystem,
) -> PaginationLayout {
    let block_spacing = block_spacing.max(0.0);

    let mut pages: Vec<PageRange> = Vec::new();
    let mut page_start = 0usize;
    let mut page_start_char_offset = 0usize;
    let mut accumulated = 0.0f32;
    let mut i = 0usize;

    while i < blocks.len() {
        let block = &blocks[i];
        let is_first_on_page = i == page_start;
        let start_off = if is_first_on_page {
            page_start_char_offset
        } else {
            0
        };
        let block_h = effective_block_height(block, start_off, page_width, font_size, font_system);
        let needed = if is_first_on_page {
            block_h
        } else {
            block_spacing + block_h
        };

        if accumulated + needed <= page_height {
            // Block fits on the current page.
            accumulated += needed;
            i += 1;
        } else if is_first_on_page {
            // First block on the page but still too tall — try to split it so
            // we don't produce an infinite empty-page loop.
            let available = (page_height - accumulated).max(0.0);
            match try_split_block(
                block,
                page_width,
                font_size,
                available,
                start_off,
                font_system,
            ) {
                Some(abs_off) if abs_off > start_off => {
                    pages.push(PageRange {
                        start: page_start,
                        start_char_offset: page_start_char_offset,
                        end: i + 1,
                        end_char_offset: abs_off,
                    });
                    page_start = i;
                    page_start_char_offset = abs_off;
                    accumulated = 0.0;
                    // Re-process block `i` on the new page.
                }
                _ => {
                    // Cannot split — let it overflow rather than loop forever.
                    pages.push(PageRange {
                        start: page_start,
                        start_char_offset: page_start_char_offset,
                        end: i + 1,
                        end_char_offset: 0,
                    });
                    page_start = i + 1;
                    page_start_char_offset = 0;
                    accumulated = 0.0;
                    i += 1;
                }
            }
        } else {
            // Block doesn't fit — try to place its top portion on the current page.
            let available = (page_height - accumulated - block_spacing).max(0.0);
            match try_split_block(block, page_width, font_size, available, 0, font_system) {
                Some(abs_off) if abs_off > 0 => {
                    pages.push(PageRange {
                        start: page_start,
                        start_char_offset: page_start_char_offset,
                        end: i + 1,
                        end_char_offset: abs_off,
                    });
                    page_start = i;
                    page_start_char_offset = abs_off;
                    accumulated = 0.0;
                    // Re-process block `i` on the new page.
                }
                _ => {
                    // Nothing fits on this page — close it and start block on next page.
                    pages.push(PageRange {
                        start: page_start,
                        start_char_offset: page_start_char_offset,
                        end: i,
                        end_char_offset: 0,
                    });
                    page_start = i;
                    page_start_char_offset = 0;
                    accumulated = 0.0;
                    // Re-process block `i` on the new page.
                }
            }
        }
    }

    // Final page (may be a continuation of a split block).
    if page_start < blocks.len() {
        pages.push(PageRange {
            start: page_start,
            start_char_offset: page_start_char_offset,
            end: blocks.len(),
            end_char_offset: 0,
        });
    }

    if pages.is_empty() {
        pages.push(PageRange {
            start: 0,
            start_char_offset: 0,
            end: 0,
            end_char_offset: 0,
        });
    }

    PaginationLayout {
        page_height,
        page_width,
        pages,
    }
}

impl EpubViewer {
    /// Ensure `block_heights_cache` has a valid entry for `chapter_idx` at the
    /// given `content_width`.  If the cached entry exists and matches both the
    /// width and the current `base_font_size` it is reused; otherwise the heights
    /// are computed via shaped text measurement and stored.
    fn ensure_block_heights(&mut self, chapter_idx: usize, content_width: f32) {
        let font_size = self.base_font_size;
        if let Some((cw, fs, _)) = self.block_heights_cache.get(&chapter_idx)
            && (*cw - content_width).abs() < 0.5
            && (*fs - font_size).abs() < 0.001
        {
            return;
        }
        let heights = match self.chapters.get(chapter_idx) {
            Some(ch) => {
                let mut fs_guard = self.font_system.borrow_mut();
                ch.blocks
                    .iter()
                    .map(|block| {
                        estimated_block_height_for_width(
                            block,
                            content_width,
                            font_size,
                            &mut fs_guard,
                        )
                    })
                    .collect()
            }
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
        let (vw, vh) = self.viewport_size.get();
        if vw <= 0.0 || vh <= 0.0 {
            return;
        }

        let space_s = theme::active().cosmic().spacing.space_s as f32;
        let space_xxs = theme::active().cosmic().spacing.space_xxs as f32;
        let page_margin = self.page_margin;
        let max_content_width = 800.0;
        // In dual-page mode, each page gets half the width minus the gap.
        let per_page_width = if self.should_dual_page(vw) {
            let available = vw - space_xxs; // gap between pages
            (available / 2.0 - space_s * 2.0).min(max_content_width)
        } else {
            max_content_width.min(vw - space_s * 2.0)
        };
        // Subtract the page margin (applied to all 4 sides of the inner content container).
        let content_width = (per_page_width - page_margin * 2.0).max(1.0);
        // Reserve space for the page indicator line and outer layout spacing.
        // Apply the user-configurable height fraction to leave extra headroom.
        let content_height = (vh - page_margin * 2.0 - space_s * 2.0 - 24.0).max(1.0);

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
                self.base_font_size,
                space_xxs,
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
        let content_w = 800.0;
        let mut fs = self.font_system.borrow_mut();
        let mut y = 0.0f32;
        for (i, block) in chapter.blocks.iter().enumerate() {
            let h =
                estimated_block_height_for_width(block, content_w, self.base_font_size, &mut fs);
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
                let content_w = 800.0;
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

    use super::load_epub_chapters;
    use super::render_chapter_blocks;
    use super::test_helper::EpubBuilder;

    #[golden_test(600, 150)]
    fn epub_plain_paragraph() -> cosmic::Element<'_, super::EpubViewerMessage> {
        let _f = EpubBuilder::new("Test")
            .body("<p>This is a plain paragraph of body text.</p>")
            .build();
        let (_, chapters, _) = load_epub_chapters(_f.path()).unwrap();
        render_chapter_blocks(&chapters, 0, 16.0)
    }

    #[golden_test(600, 150, dark)]
    fn epub_plain_paragraph_dark() -> cosmic::Element<'_, super::EpubViewerMessage> {
        let _f = EpubBuilder::new("Test")
            .body("<p>This is a plain paragraph of body text.</p>")
            .build();
        let (_, chapters, _) = load_epub_chapters(_f.path()).unwrap();
        render_chapter_blocks(&chapters, 0, 16.0)
    }

    #[golden_test(600, 300)]
    fn epub_headings() -> cosmic::Element<'_, super::EpubViewerMessage> {
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
        let _f = EpubBuilder::new("Preformatted")
            .body("<pre>fn hello() {\n    println!(\"Hello, world!\");\n}</pre>")
            .build();
        let (_, chapters, _) = load_epub_chapters(_f.path()).unwrap();
        render_chapter_blocks(&chapters, 0, 16.0)
    }

    #[golden_test(600, 200)]
    fn epub_blockquote() -> cosmic::Element<'_, super::EpubViewerMessage> {
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
        let _f = EpubBuilder::new("HR")
            .body("<p>Before the rule.</p><hr/><p>After the rule.</p>")
            .build();
        let (_, chapters, _) = load_epub_chapters(_f.path()).unwrap();
        render_chapter_blocks(&chapters, 0, 16.0)
    }

    #[golden_test(600, 200)]
    fn epub_footnote() -> cosmic::Element<'_, super::EpubViewerMessage> {
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
        // Use a small inline SVG as the figure content so no external resource
        // is needed while still exercising the Figure + caption render path.
        let _f = EpubBuilder::new("Figure")
            .body(
                "<figure>\
                   <svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 60\">\
                     <rect width=\"100\" height=\"60\" fill=\"#4080c0\"/>\
                     <text x=\"50\" y=\"35\" text-anchor=\"middle\" fill=\"white\" font-size=\"14\">SVG</text>\
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
        let _f = EpubBuilder::new("Image")
            .body("<p>An image follows:</p><img src=\"images/test.png\" alt=\"Test image\"/>")
            .resource("OEBPS/images/test.png", TINY_PNG.to_vec(), "image/png")
            .build();
        let (_, chapters, _) = load_epub_chapters(_f.path()).unwrap();
        render_chapter_blocks(&chapters, 0, 16.0)
    }

    #[golden_test(600, 200)]
    fn epub_image_data_url() -> cosmic::Element<'_, super::EpubViewerMessage> {
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
        // Image whose src cannot be resolved — falls back to alt text.
        let _f = EpubBuilder::new("Image Alt")
            .body("<img src=\"images/missing.png\" alt=\"Missing image\"/>")
            .build();
        let (_, chapters, _) = load_epub_chapters(_f.path()).unwrap();
        render_chapter_blocks(&chapters, 0, 16.0)
    }

    #[golden_test(600, 300)]
    fn epub_svg() -> cosmic::Element<'_, super::EpubViewerMessage> {
        let _f = EpubBuilder::new("SVG")
            .body(
                "<p>An inline SVG:</p>\
                 <svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 200 100\">\
                   <rect width=\"200\" height=\"100\" rx=\"10\" fill=\"#6060c0\"/>\
                   <circle cx=\"100\" cy=\"50\" r=\"30\" fill=\"#ffffff\" opacity=\"0.6\"/>\
                   <text x=\"100\" y=\"55\" text-anchor=\"middle\" fill=\"#6060c0\" font-size=\"18\">SVG</text>\
                 </svg>",
            )
            .build();
        let (_, chapters, _) = load_epub_chapters(_f.path()).unwrap();
        render_chapter_blocks(&chapters, 0, 16.0)
    }
}

/// Height estimation accuracy tests.
///
/// Each test measures a block with [`estimated_block_height_for_width`] and
/// compares it against the height computed with the **same** font metrics that
/// the renderer actually uses (COSMIC typography constants or `font_size`).
/// The invariant is:
///   `estimated >= actual_rendered`  — never underestimate (causes overflow)
///   `estimated <= actual_rendered * 1.20`  — not more than 20 % over-estimate
///
/// The "actual_rendered" value is derived by calling
/// [`measure_text_height_with_line_height`] with the exact sizes and line
/// heights that the COSMIC text widgets use (title1..heading/body).
#[cfg(test)]
mod height_tests {
    use cosmic_text::FontSystem;
    use epub::BlockStyle;
    use epub::ContentBlock;
    use rstest::rstest;

    use super::COSMIC_BODY_LINE_HEIGHT;
    use super::COSMIC_HEADING_LINE_HEIGHT;
    use super::COSMIC_HEADING_SIZE;
    use super::COSMIC_TITLE1_LINE_HEIGHT;
    use super::COSMIC_TITLE1_SIZE;
    use super::COSMIC_TITLE2_LINE_HEIGHT;
    use super::COSMIC_TITLE2_SIZE;
    use super::COSMIC_TITLE3_LINE_HEIGHT;
    use super::COSMIC_TITLE3_SIZE;
    use super::COSMIC_TITLE4_LINE_HEIGHT;
    use super::COSMIC_TITLE4_SIZE;
    use super::estimated_block_height_for_width;
    use super::measure_text_height_with_line_height;

    fn make_font_system() -> FontSystem {
        FontSystem::new()
    }

    fn plain_heading(level: u8, text: &str) -> ContentBlock {
        ContentBlock::Heading {
            level,
            text: text.to_string(),
            spans: vec![],
            style: BlockStyle::default(),
        }
    }

    fn plain_paragraph(text: &str) -> ContentBlock {
        ContentBlock::Paragraph {
            text: text.to_string(),
            spans: vec![],
            style: BlockStyle::default(),
        }
    }

    /// Helper: assert estimated is within [actual, actual * 1.20].
    fn assert_height(label: &str, estimated: f32, actual: f32) {
        assert!(
            estimated >= actual,
            "{label}: estimated {estimated:.1} < actual {actual:.1} — underestimate causes page overflow"
        );
        assert!(
            estimated <= actual * 1.20,
            "{label}: estimated {estimated:.1} > actual {actual:.1} * 1.20 = {:.1} — over-estimate wastes space",
            actual * 1.20
        );
    }

    const WIDTH: f32 = 600.0;
    const BASE_FONT_SIZE: f32 = 14.0;

    // ── Plain heading height matches COSMIC typography ────────────────────────

    #[rstest]
    #[case(1, COSMIC_TITLE1_SIZE, COSMIC_TITLE1_LINE_HEIGHT)]
    #[case(2, COSMIC_TITLE2_SIZE, COSMIC_TITLE2_LINE_HEIGHT)]
    #[case(3, COSMIC_TITLE3_SIZE, COSMIC_TITLE3_LINE_HEIGHT)]
    #[case(4, COSMIC_TITLE4_SIZE, COSMIC_TITLE4_LINE_HEIGHT)]
    #[case(5, COSMIC_HEADING_SIZE, COSMIC_HEADING_LINE_HEIGHT)]
    #[case(6, COSMIC_HEADING_SIZE, COSMIC_HEADING_LINE_HEIGHT)]
    fn plain_heading_single_line(
        #[case] level: u8,
        #[case] cosmic_size: f32,
        #[case] cosmic_line_h: f32,
    ) {
        let mut fs = make_font_system();
        let text = "Chapter Title";
        let block = plain_heading(level, text);
        let estimated = estimated_block_height_for_width(&block, WIDTH, BASE_FONT_SIZE, &mut fs);
        let actual =
            measure_text_height_with_line_height(text, WIDTH, cosmic_size, cosmic_line_h, &mut fs);
        assert_height(&format!("h{level} single-line"), estimated, actual);
    }

    #[rstest]
    #[case(1, COSMIC_TITLE1_SIZE, COSMIC_TITLE1_LINE_HEIGHT)]
    #[case(2, COSMIC_TITLE2_SIZE, COSMIC_TITLE2_LINE_HEIGHT)]
    #[case(3, COSMIC_TITLE3_SIZE, COSMIC_TITLE3_LINE_HEIGHT)]
    fn plain_heading_multiline(
        #[case] level: u8,
        #[case] cosmic_size: f32,
        #[case] cosmic_line_h: f32,
    ) {
        let mut fs = make_font_system();
        // Text long enough to wrap at 600px for the largest heading sizes.
        let text = "A Very Long Chapter Title That Should Wrap Across Multiple Lines On The Page";
        let block = plain_heading(level, text);
        let estimated = estimated_block_height_for_width(&block, WIDTH, BASE_FONT_SIZE, &mut fs);
        let actual =
            measure_text_height_with_line_height(text, WIDTH, cosmic_size, cosmic_line_h, &mut fs);
        assert_height(&format!("h{level} multi-line"), estimated, actual);
    }

    // ── Plain paragraph height matches COSMIC body typography ─────────────────

    #[test]
    fn plain_paragraph_single_line() {
        let mut fs = make_font_system();
        let text = "A short paragraph.";
        let block = plain_paragraph(text);
        let estimated = estimated_block_height_for_width(&block, WIDTH, BASE_FONT_SIZE, &mut fs);
        let actual = measure_text_height_with_line_height(
            text,
            WIDTH,
            BASE_FONT_SIZE,
            COSMIC_BODY_LINE_HEIGHT,
            &mut fs,
        );
        assert_height("paragraph single-line", estimated, actual);
    }

    #[test]
    fn plain_paragraph_multiline() {
        let mut fs = make_font_system();
        let text = "This is a longer paragraph that contains enough text to wrap across \
                    multiple lines when rendered at the default content width. It exercises \
                    the multi-line height estimation path.";
        let block = plain_paragraph(text);
        let estimated = estimated_block_height_for_width(&block, WIDTH, BASE_FONT_SIZE, &mut fs);
        let actual = measure_text_height_with_line_height(
            text,
            WIDTH,
            BASE_FONT_SIZE,
            COSMIC_BODY_LINE_HEIGHT,
            &mut fs,
        );
        assert_height("paragraph multi-line", estimated, actual);
    }

    // ── Heading height scales correctly with content width ────────────────────

    #[rstest]
    #[case(300.0)]
    #[case(600.0)]
    #[case(800.0)]
    fn h1_height_at_various_widths(#[case] width: f32) {
        let mut fs = make_font_system();
        let text = "A Chapter Title Long Enough To Potentially Wrap At Narrow Widths";
        let block = plain_heading(1, text);
        let estimated = estimated_block_height_for_width(&block, width, BASE_FONT_SIZE, &mut fs);
        let actual = measure_text_height_with_line_height(
            text,
            width,
            COSMIC_TITLE1_SIZE,
            COSMIC_TITLE1_LINE_HEIGHT,
            &mut fs,
        );
        assert_height(&format!("h1 width={width}"), estimated, actual);
    }
}

#[cfg(test)]
mod split_tests {
    use cosmic_text::FontSystem;
    use rstest::rstest;

    use super::find_split_char_offset;

    fn make_font_system() -> FontSystem {
        FontSystem::new()
    }

    const WIDTH: f32 = 600.0;
    const FONT_SIZE: f32 = 14.0;
    /// One line of body text at FONT_SIZE.
    const ONE_LINE_HEIGHT: f32 = FONT_SIZE * 1.375;

    // ── Early-exit conditions ────────────────────────────────────────────────

    #[test]
    fn empty_text_returns_none() {
        let mut fs = make_font_system();
        assert_eq!(
            find_split_char_offset("", WIDTH, FONT_SIZE, 100.0, &mut fs),
            None
        );
    }

    #[test]
    fn zero_height_returns_none() {
        let mut fs = make_font_system();
        assert_eq!(
            find_split_char_offset("some text", WIDTH, FONT_SIZE, 0.0, &mut fs),
            None
        );
    }

    #[test]
    fn negative_height_returns_none() {
        let mut fs = make_font_system();
        assert_eq!(
            find_split_char_offset("some text", WIDTH, FONT_SIZE, -1.0, &mut fs),
            None
        );
    }

    /// The function returns `None` only when *zero* lines fit (height is too
    /// small for even the first line).  When all lines fit it returns
    /// `Some(end_of_text)`.  Either way the result must be in bounds and must
    /// not panic.
    #[test]
    fn no_lines_fit_returns_none() {
        let mut fs = make_font_system();
        // 1 px is less than any realistic line height, so the very first layout
        // run exceeds it and we break immediately with last_byte_end = None.
        assert_eq!(
            find_split_char_offset("Short text.", WIDTH, FONT_SIZE, 1.0, &mut fs),
            None
        );
    }

    #[test]
    fn large_height_does_not_panic_and_result_is_in_bounds() {
        let text = "Short text.";
        let mut fs = make_font_system();
        // A huge height: either None (zero lines fit, unusual) or Some(n ≤ len).
        if let Some(offset) = find_split_char_offset(text, WIDTH, FONT_SIZE, 10_000.0, &mut fs) {
            assert!(offset <= text.chars().count());
        }
    }

    // ── Basic split contract ─────────────────────────────────────────────────

    #[rstest]
    #[case(ONE_LINE_HEIGHT)]
    #[case(ONE_LINE_HEIGHT * 2.0)]
    #[case(ONE_LINE_HEIGHT * 3.0)]
    fn split_offset_is_in_bounds(#[case] height: f32) {
        let text = "alpha bravo charlie delta echo foxtrot golf hotel india juliet kilo \
                    lima mike november oscar papa quebec romeo sierra tango uniform victor";
        let mut fs = make_font_system();
        if let Some(offset) = find_split_char_offset(text, WIDTH, FONT_SIZE, height, &mut fs) {
            let char_count = text.chars().count();
            assert!(offset > 0, "split offset must be > 0, got {offset}");
            assert!(
                offset < char_count,
                "split offset {offset} must be < char count {char_count}"
            );
        }
    }

    // ── Multibyte / char-boundary safety (regression for the crash) ──────────

    /// The exact text from the crash report.  Curly-quote characters (\u{201C}/\u{201D})
    /// are 3-byte UTF-8 sequences; cosmic-text glyph.end can land mid-codepoint.
    /// Before the fix this panicked with "byte index N is not a char boundary".
    #[test]
    fn multibyte_curly_quotes_dont_panic() {
        let text = "guards.\n Guards specify conditions that a request must satisfy in \
                    order to \u{201C}match\u{201D} and be passed over to the handler. \
                    From an implementation standpoint guards are implementors of the \
                    Guard trait: Guard::check is where the magic happens.";
        let mut fs = make_font_system();
        // Tiny height so the split triggers near the curly quote.
        let _ = find_split_char_offset(text, WIDTH, FONT_SIZE, ONE_LINE_HEIGHT * 1.5, &mut fs);
    }

    /// Any `Some(offset)` returned must correspond to a valid UTF-8 char boundary
    /// when converted back to a byte index.
    #[test]
    fn split_result_lands_on_char_boundary() {
        let text = "guards. Guards specify conditions that a request must satisfy in \
                    order to \u{201C}match\u{201D} and be passed over to the handler. \
                    From an implementation standpoint guards are implementors of the \
                    Guard trait: Guard::check is where the magic happens.";
        let mut fs = make_font_system();
        for available_height in [
            ONE_LINE_HEIGHT,
            ONE_LINE_HEIGHT * 2.0,
            ONE_LINE_HEIGHT * 3.0,
        ] {
            if let Some(offset) =
                find_split_char_offset(text, WIDTH, FONT_SIZE, available_height, &mut fs)
            {
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

    // ── Word-boundary snap ───────────────────────────────────────────────────

    /// When the raw split falls mid-word, the returned prefix (first `offset` chars)
    /// must end with whitespace — i.e. the split is at a word boundary.
    #[test]
    fn split_does_not_break_inside_word() {
        let text = "alpha bravo charlie delta echo foxtrot golf hotel india juliet";
        let mut fs = make_font_system();
        // Narrow width forces wrapping and a mid-word raw split.
        if let Some(offset) =
            find_split_char_offset(text, 200.0, FONT_SIZE, ONE_LINE_HEIGHT, &mut fs)
        {
            let prefix: String = text.chars().take(offset).collect();
            assert!(
                prefix.ends_with(char::is_whitespace),
                "split prefix should end with whitespace, got: {prefix:?}"
            );
        }
    }

    /// A single very long word with no preceding whitespace must not loop
    /// forever; the function falls back to a raw char offset.
    #[test]
    fn single_long_word_returns_raw_offset() {
        // 200 'a' chars — no whitespace at all.
        let text: String = "a".repeat(200);
        let mut fs = make_font_system();
        // If any lines overflow, the offset must still be in bounds.
        if let Some(offset) =
            find_split_char_offset(&text, WIDTH, FONT_SIZE, ONE_LINE_HEIGHT, &mut fs)
        {
            assert!(offset > 0 && offset < text.chars().count());
        }
    }
}
