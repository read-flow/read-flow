//! Paginated mode must fill each page down to the bottom margin without
//! overflowing it, and break paragraphs exactly at line boundaries.
//!
//! These tests lay out the real paginated view with the headless renderer and
//! inspect the page scrollables, so they check what the reader actually sees
//! rather than the pagination's own estimates.

use cosmic::iced::Font;
use cosmic::iced::Pixels;
use cosmic::iced::Size;
use cosmic::iced::advanced::graphics::text::Paragraph;
use cosmic::iced::advanced::text::Paragraph as _;
use cosmic::iced::alignment;
use cosmic::iced::font;
use cosmic::iced::widget::text::LineHeight;
use cosmic::iced::widget::text::Shaping;
use cosmic::iced::widget::text::Span;
use cosmic::iced::widget::text::Wrapping;
use epub::ContentBlock;

use super::DualPageMode;
use super::EpubViewer;
use super::FontFamily;
use super::Page as _;
use super::ViewMode;
use super::char_offset_to_byte;
use super::page_text_id;
use super::test_helper::EpubBuilder;

const FONT_SIZE: f32 = 16.0;
const PAGE_MARGIN: f32 = 16.0;
/// Height of one line of body text: the renderer's 1.4 line height.
const LINE_ADVANCE: f32 = FONT_SIZE * 1.4;

/// Viewport sizes covering the layout variants: narrow single page (fixed
/// page-turn zones), wide single page, and dual page.
const VIEWPORTS: [(f32, f32); 3] = [(760.0, 600.0), (1100.0, 700.0), (1500.0, 700.0)];

const WORDS: &[&str] = &[
    "the",
    "psychedelic",
    "experience",
    "is",
    "a",
    "journey",
    "to",
    "new",
    "realms",
    "of",
    "consciousness",
    "whose",
    "scope",
    "and",
    "content",
    "are",
    "limitless",
    "transcendence",
    "verbal",
    "concepts",
    "space-time",
    "dimensions",
    "ego",
    "identity",
    "enlarged",
    "occur",
    "in",
    "variety",
    "ways",
];

/// Deterministic prose of `words` words.
fn prose(seed: usize, words: usize) -> String {
    (0..words)
        .map(|i| WORDS[(seed * 7 + i * i * 3 + i) % WORDS.len()])
        .collect::<Vec<_>>()
        .join(" ")
}

/// A chapter of long paragraphs, so pages break inside paragraphs.
fn long_chapter(styled: bool) -> String {
    (0..8)
        .map(|p| {
            let text = prose(p, 180 + p * 23);
            if styled && p % 2 == 1 {
                let cut = text[..text.len() / 3].rfind(' ').expect("several words");
                let (head, tail) = text.split_at(cut);
                format!("<p><em>{head}</em><b>{tail}</b></p>")
            } else {
                format!("<p>{text}</p>")
            }
        })
        .collect()
}

/// A viewer in paginated mode with fixed preferences (not the user's saved
/// ones), with `body` loaded as its only chapter.
async fn paginated_viewer(
    body: String,
) -> (
    EpubViewer,
    tempfile::NamedTempFile,
    tempfile::TempDir,
    tempfile::TempDir,
) {
    let (application_module, document_provider, db_dir) =
        crate::test_support::document_provider().await;
    let epub_file = EpubBuilder::new("Pagination").body(body).build();
    let (document, fixture_dir) = crate::test_support::scan_and_fetch_document(
        &application_module,
        &document_provider,
        epub_file.path().to_path_buf(),
        "pagination.epub",
    )
    .await;
    let (mut viewer, init_task) = EpubViewer::new(document, document_provider, false);
    for message in crate::test_support::drain(init_task).await {
        if matches!(message, super::EpubViewerMessage::EpubLoaded(..)) {
            let _ = viewer.update(message);
        }
    }
    viewer.view_mode = ViewMode::Paginated;
    viewer.dual_page = DualPageMode::Auto;
    viewer.font_family = FontFamily::Serif;
    viewer.base_font_size = FONT_SIZE;
    viewer.page_margin = PAGE_MARGIN;
    viewer.pagination_cache.clear();
    viewer.current_page = 0;
    (viewer, epub_file, db_dir, fixture_dir)
}

/// Where one shown page's text ends relative to the page's visible area.
#[derive(Debug)]
struct PageFit {
    /// Visible text area of the page (its scrollable viewport).
    viewport: cosmic::iced::Rectangle,
    /// The page's text column, including its margins.
    text: cosmic::iced::Rectangle,
}

impl PageFit {
    /// Space left between the end of the text and the bottom of the page;
    /// negative when the text overflows.
    fn gap(&self) -> f32 {
        (self.viewport.y + self.viewport.height) - (self.text.y + self.text.height)
    }
}

/// Lay out the paginated view showing `page` and return how each shown page
/// (one, or two side by side) fits.
fn page_fits(viewer: &mut EpubViewer, page: usize, (width, height): (f32, f32)) -> Vec<PageFit> {
    viewer.current_page = page;
    let extents =
        crate::test_support::layout_extents(viewer.view_content_paginated(), width, height);
    let texts: Vec<_> = extents
        .containers
        .iter()
        .filter(|(id, _)| *id == page_text_id())
        .map(|(_, bounds)| *bounds)
        .collect();
    assert_eq!(
        texts.len(),
        extents.scrollables.len(),
        "one text column per page"
    );
    extents
        .scrollables
        .iter()
        .zip(texts)
        .map(|(scrollable, text)| PageFit {
            viewport: scrollable.viewport,
            text,
        })
        .collect()
}

/// Paginate the active chapter for `viewport` the way the app does: one
/// layout pass to learn the viewport, then the pagination update.
fn paginate(viewer: &mut EpubViewer, viewport: (f32, f32)) -> usize {
    viewer.pagination_cache.clear();
    let _ = page_fits(viewer, 0, viewport);
    viewer.maybe_repaginate();
    viewer.total_pages()
}

#[tokio::test]
async fn pages_fill_to_the_bottom_margin_without_overflowing() {
    for styled in [false, true] {
        let (mut viewer, ..) = paginated_viewer(long_chapter(styled)).await;
        let _guard = crate::test_support::cosmic_tk_lock();
        cosmic_golden::init();
        for viewport in VIEWPORTS {
            let total = paginate(&mut viewer, viewport);
            assert!(
                total > 2,
                "{viewport:?}: expected several pages, got {total}"
            );
            let spacing = cosmic::theme::active().cosmic().spacing.space_xxs as f32;
            let mut page = 0;
            while page < total {
                let fits = page_fits(&mut viewer, page, viewport);
                assert!(!fits.is_empty(), "{viewport:?} page {page}: no page shown");
                for (side, fit) in fits.iter().enumerate() {
                    let shown = page + side;
                    let gap = fit.gap();
                    assert!(
                        gap >= -0.5,
                        "styled={styled} {viewport:?} page {shown}: text overflows by {:.1}px",
                        -gap
                    );
                    if shown + 1 < total {
                        assert!(
                            gap < LINE_ADVANCE + spacing,
                            "styled={styled} {viewport:?} page {shown}: bottom gap {gap:.1}px \
                             leaves room for another line"
                        );
                    }
                }
                page += fits.len();
            }
        }
    }
}

/// Number of lines `text` takes when rendered as body text `width` wide.
fn line_count(text: &str, width: f32) -> usize {
    let spans = [Span::<'_, (), Font>::new(text).font(Font {
        family: font::Family::Serif,
        ..Font::default()
    })];
    let paragraph = Paragraph::with_spans(cosmic::iced::advanced::Text {
        content: &spans[..],
        bounds: Size::new(width, f32::INFINITY),
        size: Pixels(FONT_SIZE),
        line_height: LineHeight::Relative(1.4),
        font: Font::default(),
        align_x: cosmic::iced::advanced::text::Alignment::Default,
        align_y: alignment::Vertical::Top,
        shaping: Shaping::Advanced,
        wrapping: Wrapping::Word,
        ellipsize: Default::default(),
    });
    paragraph.buffer().layout_runs().count()
}

#[tokio::test]
async fn page_breaks_leave_no_room_on_the_last_line() {
    let (mut viewer, ..) = paginated_viewer(long_chapter(false)).await;
    let _guard = crate::test_support::cosmic_tk_lock();
    cosmic_golden::init();
    for viewport in VIEWPORTS {
        let total = paginate(&mut viewer, viewport);
        let text_width = page_fits(&mut viewer, 0, viewport)[0].text.width - 2.0 * PAGE_MARGIN;
        let chapter = &viewer.chapters[viewer.active_chapter];
        let layout = &viewer.pagination_cache[&viewer.active_chapter];
        let mut breaks = 0;
        for (page, range) in layout.pages.iter().enumerate().take(total - 1) {
            if range.end_char_offset == 0 {
                continue;
            }
            let ContentBlock::Paragraph { text, .. } = &chapter.blocks[range.end - 1] else {
                continue;
            };
            breaks += 1;
            let start = if range.end - 1 == range.start {
                range.start_char_offset
            } else {
                0
            };
            let on_page = &text[char_offset_to_byte(text, start)
                ..char_offset_to_byte(text, range.end_char_offset)];
            let rest = &text[char_offset_to_byte(text, range.end_char_offset)..];
            let next_word = rest.split_whitespace().next().expect("text continues");
            let with_next = format!("{} {next_word}", on_page.trim_end());
            assert!(
                line_count(&with_next, text_width) > line_count(on_page, text_width),
                "{viewport:?} page {page}: \"{next_word}\" still fits on the last line \
                 (page ends with {:?})",
                &on_page[on_page.len().saturating_sub(40)..]
            );
        }
        assert!(
            breaks > 0,
            "{viewport:?}: expected a page break inside a paragraph"
        );
    }
}

/// A chapter as books typically have it: a heading, then long paragraphs,
/// some with styled runs.
fn book_chapter() -> String {
    format!("<h2>I. General Introduction</h2>{}", long_chapter(true))
}

/// Golden snapshots of paginated pages: the text must fill each page down to
/// the bottom margin, with full last lines and no scrollbar.
#[tokio::test]
async fn paginated_page_goldens() {
    let (mut viewer, ..) = paginated_viewer(book_chapter()).await;
    let _guard = crate::test_support::cosmic_tk_lock();
    cosmic_golden::init();
    for (name, viewport) in [
        ("paginated_single_narrow", VIEWPORTS[0]),
        ("paginated_single_wide", VIEWPORTS[1]),
        ("paginated_dual", VIEWPORTS[2]),
    ] {
        let total = paginate(&mut viewer, viewport);
        let (width, height) = (viewport.0 as u32, viewport.1 as u32);
        for (suffix, page) in [("first", 0), ("middle", total / 2)] {
            viewer.current_page = page;
            cosmic_golden::assert_snapshot!(
                &format!("{name}_{suffix}"),
                viewer.view_content_paginated(),
                width,
                height
            );
        }
    }
}
