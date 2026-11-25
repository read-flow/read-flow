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
use super::EpubViewerMessage;
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
    paginated_viewer_with_resources(body, Vec::new()).await
}

/// [`paginated_viewer`] for a book that also holds `resources` (zip path,
/// data, media type).
async fn paginated_viewer_with_resources(
    body: String,
    resources: Vec<(&str, Vec<u8>, &'static str)>,
) -> (
    EpubViewer,
    tempfile::NamedTempFile,
    tempfile::TempDir,
    tempfile::TempDir,
) {
    let (application_module, document_provider, db_dir) =
        crate::test_support::document_provider().await;
    let mut builder = EpubBuilder::new("Pagination").body(body);
    for (path, data, media_type) in resources {
        builder = builder.resource(path, data, media_type);
    }
    let epub_file = builder.build();
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
            assert_pages_fill(&mut viewer, viewport, &format!("styled={styled}"));
        }
    }
}

#[tokio::test]
async fn justify_toggle_updates_both_render_modes_and_keeps_the_page() {
    let (mut viewer, ..) = paginated_viewer(long_chapter(false)).await;
    let _guard = crate::test_support::cosmic_tk_lock();
    cosmic_golden::init();
    let viewport = VIEWPORTS[0];
    let total = paginate(&mut viewer, viewport);
    assert!(total > 1);
    viewer.current_page = 1;
    let chapter_index = viewer.active_chapter;
    let cuts = |viewer: &EpubViewer| {
        viewer.pagination_cache[&chapter_index]
            .pages
            .iter()
            .map(|page| (page.start, page.start_cut, page.end, page.end_cut))
            .collect::<Vec<_>>()
    };
    let original_cuts = cuts(&viewer);

    let _ = viewer.update(EpubViewerMessage::SetJustifyParagraphs(true));
    let chapter = &viewer.chapters[viewer.active_chapter];
    assert!(viewer.scroll_render_context(chapter).justify_paragraphs);
    assert!(
        viewer
            .page_render_context(chapter, viewport.1)
            .justify_paragraphs
    );
    assert_eq!(viewer.current_page, 1);
    assert_eq!(viewer.total_pages(), total);
    assert_eq!(cuts(&viewer), original_cuts);

    viewer.view_mode = ViewMode::Scroll;
    viewer.scroll_y = 120.0;
    let expected_heights = viewer.active_scroll_heights().unwrap().to_vec();
    // Empty the cached heights so the assertion below requires a rebuild.
    viewer
        .block_heights_cache
        .get_mut(&chapter_index)
        .unwrap()
        .2
        .clear();

    let _ = viewer.update(EpubViewerMessage::SetJustifyParagraphs(false));
    let chapter = &viewer.chapters[viewer.active_chapter];
    assert!(!viewer.scroll_render_context(chapter).justify_paragraphs);
    assert!(
        !viewer
            .page_render_context(chapter, viewport.1)
            .justify_paragraphs
    );
    assert_eq!(viewer.scroll_y, 120.0);
    // Matching heights keep the same scroll offset anchored in the same text.
    assert_eq!(viewer.active_scroll_heights().unwrap(), expected_heights);
    assert_eq!(viewer.current_page, 1);
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
            let Some(end_cut) = range.end_cut else {
                continue;
            };
            let ContentBlock::Paragraph { text, .. } = &chapter.blocks()[range.end - 1] else {
                continue;
            };
            breaks += 1;
            let start = if range.end - 1 == range.start {
                range.start_cut.char_offset
            } else {
                0
            };
            let on_page = &text
                [char_offset_to_byte(text, start)..char_offset_to_byte(text, end_cut.char_offset)];
            let rest = &text[char_offset_to_byte(text, end_cut.char_offset)..];
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

#[tokio::test]
#[ignore = "golden snapshot differs by ~281 px on Linux CI (font rendering); regenerate the goldens on Linux, then remove this attribute"]
async fn epub_read_flow_chrome_goldens() {
    let (mut viewer, ..) = paginated_viewer(book_chapter()).await;
    let _guard = crate::test_support::cosmic_tk_lock();
    cosmic_golden::init();

    let header = viewer.view_header_start().into_iter().fold(
        cosmic::widget::header_bar().focused(true),
        |header, element| header.start(element),
    );
    let header = viewer
        .view_header_center()
        .into_iter()
        .fold(header, |header, element| header.center(element));
    let header = viewer
        .view_header_end()
        .into_iter()
        .fold(header, |header, element| header.end(element));
    let header: cosmic::Element<'_, EpubViewerMessage> = header.into();
    cosmic_golden::assert_snapshot!("epub_read_flow_header", header, 960, 64);

    for (name, mode) in [
        ("epub_read_flow_context_paginated", ViewMode::Paginated),
        ("epub_read_flow_context_scroll", ViewMode::Scroll),
    ] {
        viewer.view_mode = mode;
        let context = viewer.view_context();
        let drawer: cosmic::Element<'_, EpubViewerMessage> =
            cosmic::widget::ContextDrawer::new_inner(
                Some(context.title.into()),
                None,
                None,
                None,
                context.content,
                EpubViewerMessage::RequestClose,
                420.0,
            )
            .into();
        cosmic_golden::assert_snapshot!(name, drawer, 420, 900);
    }
}

/// A chapter of `count` blocks cycling through every block type.
fn mixed_chapter(count: usize) -> String {
    (0..count)
        .map(|i| match i % 12 {
            0 => format!("<h2>Section {i}</h2>"),
            1 | 2 => format!("<p>{}</p>", prose(i, 60 + i % 40)),
            3 => format!("<p><em>{}</em> {}</p>", prose(i, 10), prose(i + 1, 40)),
            4 => format!("<ul><li>{}</li><li>{}</li></ul>", prose(i, 8), prose(i, 20)),
            5 => format!("<ol><li>{}</li><li>{}</li></ol>", prose(i, 8), prose(i, 20)),
            6 => format!(
                "<blockquote><p>{}</p><p>{}</p></blockquote>",
                prose(i, 30),
                prose(i, 5)
            ),
            // One line, so it is never split across pages.
            7 => "<pre>let answer = 42;</pre>".to_string(),
            8 => "<table><tr><th>A</th><th>B</th></tr><tr><td>1</td><td>2</td></tr></table>"
                .to_string(),
            9 => "<hr/>".to_string(),
            10 => "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 200 100\">\
                   <rect width=\"200\" height=\"100\" fill=\"#6060c0\"/></svg>"
                .to_string(),
            _ => format!(
                "<figure><svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 60\">\
                 <rect width=\"100\" height=\"60\"/></svg><figcaption>{}</figcaption></figure>",
                prose(i, 6)
            ),
        })
        .collect()
}

/// Heights of the active chapter's page texts, checking that none overflows
/// its page.
fn page_text_heights(viewer: &mut EpubViewer, viewport: (f32, f32)) -> Vec<f32> {
    let total = viewer.total_pages();
    let mut heights = Vec::new();
    let mut page = 0;
    while page < total {
        let fits = page_fits(viewer, page, viewport);
        for (side, fit) in fits.iter().enumerate() {
            let gap = fit.gap();
            assert!(
                gap >= -0.5,
                "{viewport:?} page {}: text overflows by {:.1}px",
                page + side,
                -gap
            );
            heights.push(fit.text.height);
        }
        page += fits.len();
    }
    heights
}

#[tokio::test]
async fn font_change_repaginates_all_block_types() {
    let (mut viewer, ..) = paginated_viewer(mixed_chapter(48)).await;
    let _guard = crate::test_support::cosmic_tk_lock();
    cosmic_golden::init();
    let viewport = VIEWPORTS[0];
    paginate(&mut viewer, viewport);
    let pages_before = viewer.total_pages();

    for (change, message) in [
        (
            "font family",
            super::EpubViewerMessage::SetFontFamily(FontFamily::SansSerif),
        ),
        (
            "font size",
            super::EpubViewerMessage::SetBaseFontSize(FONT_SIZE + 4.0),
        ),
        (
            "page margin",
            super::EpubViewerMessage::SetPageMargin(PAGE_MARGIN * 3.0),
        ),
    ] {
        let previous = (
            viewer.total_pages(),
            page_text_heights(&mut viewer, viewport),
        );
        let _ = viewer.update(message);
        let after = (
            viewer.total_pages(),
            page_text_heights(&mut viewer, viewport),
        );
        assert_ne!(after, previous, "{change}: pages not recomputed");
    }
    assert!(
        viewer.total_pages() > pages_before,
        "larger text and margins take more pages: {pages_before} then {}",
        viewer.total_pages()
    );
}

#[rstest::rstest]
#[case::chapter(crate::page::EpubViewerMessage::SelectChapter(1))]
#[case::contents_entry(crate::page::EpubViewerMessage::SelectNavEntry(1))]
#[tokio::test]
async fn switching_chapters_paginates_the_new_chapter(#[case] message: super::EpubViewerMessage) {
    let (mut viewer, ..) = paginated_viewer(mixed_chapter(20)).await;
    let _guard = crate::test_support::cosmic_tk_lock();
    cosmic_golden::init();
    // A second chapter, listed in the table of contents.
    let mut second = viewer.chapters[0].clone();
    second.href = format!("{}-second", second.href);
    viewer.nav_entries.push(epub::NavEntry {
        href: second.href.clone(),
        label: "Second".to_string(),
        depth: 0,
    });
    viewer.chapters.push(second);
    paginate(&mut viewer, VIEWPORTS[0]);
    let _ = viewer.update(message);
    assert_eq!(viewer.active_chapter, 1);
    assert!(
        viewer.pagination_cache.contains_key(&1),
        "the update paginates the chapter it switches to, so view() need not"
    );
}

#[tokio::test]
#[cfg_attr(debug_assertions, ignore = "timing needs a release build")]
async fn paginating_a_long_chapter_is_fast() {
    let (mut viewer, ..) = paginated_viewer(mixed_chapter(1000)).await;
    let _guard = crate::test_support::cosmic_tk_lock();
    cosmic_golden::init();
    assert!(viewer.chapters[viewer.active_chapter].blocks().len() >= 1000);
    // Learn the page area, then time the pagination alone.
    paginate(&mut viewer, VIEWPORTS[1]);
    viewer.pagination_cache.clear();
    let started = std::time::Instant::now();
    viewer.maybe_repaginate();
    let elapsed = started.elapsed();
    let total = viewer.total_pages();
    assert!(total > 10, "expected many pages, got {total}");
    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "paginating {total} pages took {elapsed:?}"
    );
}

/// A paragraph, then a table with a header row and `rows` body rows.
fn long_table_chapter(rows: usize) -> String {
    let rows: String = (0..rows)
        .map(|i| format!("<tr><td>{i}</td><td>{}</td></tr>", prose(i, 2 + i % 4)))
        .collect();
    format!(
        "<p>{}</p><table><tr><th>Number</th><th>Description</th></tr>{rows}</table>",
        prose(0, 60)
    )
}

#[rstest::rstest]
#[case::table(long_table_chapter(2000))]
#[case::code_block(code_chapter(2000))]
#[tokio::test]
#[cfg_attr(debug_assertions, ignore = "timing needs a release build")]
async fn paginating_a_long_block_is_fast(#[case] chapter: String) {
    let (mut viewer, ..) = paginated_viewer(chapter).await;
    let _guard = crate::test_support::cosmic_tk_lock();
    cosmic_golden::init();
    // Learn the page area, then time the pagination alone.
    paginate(&mut viewer, VIEWPORTS[1]);
    viewer.pagination_cache.clear();
    let started = std::time::Instant::now();
    viewer.maybe_repaginate();
    let elapsed = started.elapsed();
    let total = viewer.total_pages();
    assert!(total > 20, "expected many pages, got {total}");
    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "paginating {total} pages took {elapsed:?}"
    );
}

/// Check that every page of the active chapter is filled down to its bottom
/// margin without overflowing: each page but the last leaves less room than
/// another line.
fn assert_pages_fill(viewer: &mut EpubViewer, viewport: (f32, f32), what: &str) {
    let total = viewer.total_pages();
    let spacing = cosmic::theme::active().cosmic().spacing.space_xxs as f32;
    let mut page = 0;
    while page < total {
        let fits = page_fits(viewer, page, viewport);
        assert!(
            !fits.is_empty(),
            "{what} {viewport:?} page {page}: no page shown"
        );
        for (side, fit) in fits.iter().enumerate() {
            let shown = page + side;
            let gap = fit.gap();
            assert!(
                gap >= -0.5,
                "{what} {viewport:?} page {shown}: text overflows by {:.1}px",
                -gap
            );
            if shown + 1 < total {
                // The next unit that did not fit: a line, the heading starting
                // the next page, or, when that heading would have fit but was
                // moved to keep it with the text after it, the headings
                // starting the next page with the first line after them.
                let headings = leading_headings(viewer, shown + 1);
                let bound = match headings.first() {
                    None => LINE_ADVANCE + spacing,
                    Some(&first) if gap < spacing + first => spacing + first,
                    Some(_) => {
                        headings.iter().map(|height| spacing + height).sum::<f32>()
                            + spacing
                            + LINE_ADVANCE
                    }
                };
                assert!(
                    gap < bound,
                    "{what} {viewport:?} page {shown}: bottom gap {gap:.1}px leaves room \
                     for {bound:.1}px"
                );
            }
        }
        page += fits.len();
    }
}

/// Measured heights of the headings starting page `page` of the active
/// chapter, passing over the anchors among them (which take no room); empty
/// when the page does not start with a heading, or continues a block from
/// the page before it.
fn leading_headings(viewer: &EpubViewer, page: usize) -> Vec<f32> {
    let chapter = &viewer.chapters[viewer.active_chapter];
    let layout = &viewer.pagination_cache[&viewer.active_chapter];
    let range = &layout.pages[page];
    if range.start_cut != super::BlockCut::default() {
        return Vec::new();
    }
    let ctx = viewer.page_render_context(chapter, layout.page_height);
    chapter.blocks()[range.start..]
        .iter()
        .take_while(|block| super::is_heading(block) || super::is_anchor(block))
        .filter(|block| super::is_heading(block))
        .map(|block| {
            super::block_height(
                &mut viewer.measurer.borrow_mut(),
                &ctx,
                block,
                layout.page_width,
            )
        })
        .collect()
}

/// A paragraph followed by a code block of `lines` lines, each short enough
/// to stay on one line.
fn code_chapter(lines: usize) -> String {
    let code = (0..lines)
        .map(|i| format!("let value_{i} = compute({i}, {});", i * 7 % 13))
        .collect::<Vec<_>>()
        .join("\n");
    format!("<p>{}</p><pre>{code}</pre>", prose(0, 60))
}

#[tokio::test]
async fn preformatted_pages_fill_without_overflowing() {
    let (mut viewer, ..) = paginated_viewer(code_chapter(120)).await;
    let _guard = crate::test_support::cosmic_tk_lock();
    cosmic_golden::init();
    for viewport in VIEWPORTS {
        let total = paginate(&mut viewer, viewport);
        assert!(
            total > 2,
            "{viewport:?}: expected several pages, got {total}"
        );
        assert_pages_fill(&mut viewer, viewport, "code");

        let chapter = &viewer.chapters[viewer.active_chapter];
        let layout = &viewer.pagination_cache[&viewer.active_chapter];
        for (page, range) in layout.pages.iter().enumerate() {
            let Some(end_cut) = range.end_cut else {
                continue;
            };
            let ContentBlock::Preformatted { text, .. } = &chapter.blocks()[range.end - 1] else {
                panic!("{viewport:?} page {page}: only the code block is split");
            };
            assert!(
                text[..char_offset_to_byte(text, end_cut.char_offset)].ends_with('\n'),
                "{viewport:?} page {page}: code block split inside a line"
            );
        }
    }
}

#[tokio::test]
async fn long_preformatted_line_wraps_within_the_page() {
    let line: String = (0..400)
        .map(|i| char::from(b'a' + (i % 26) as u8))
        .collect();
    let (mut viewer, ..) = paginated_viewer(format!("<pre>{line}</pre>")).await;
    let _guard = crate::test_support::cosmic_tk_lock();
    cosmic_golden::init();
    for viewport in VIEWPORTS {
        paginate(&mut viewer, viewport);
        let fit = &page_fits(&mut viewer, 0, viewport)[0];
        assert!(
            fit.gap() >= -0.5,
            "{viewport:?}: text overflows by {:.1}px",
            -fit.gap()
        );
        // Monospace glyphs are at least half an em wide, so the line takes
        // at least this many lines when it wraps within the page.
        let text_width = fit.text.width - 2.0 * PAGE_MARGIN;
        let min_lines = (line.len() as f32 * FONT_SIZE * 0.5 / text_width).ceil();
        let text_height = fit.text.height - 2.0 * PAGE_MARGIN;
        assert!(
            text_height >= min_lines * LINE_ADVANCE,
            "{viewport:?}: the line is {text_height:.1}px high, expected at least \
             {min_lines} wrapped lines"
        );
        // The code block is drawn as this layout, and its lines stay within
        // the page's text column.
        let padding = super::render::preformatted_padding();
        let spans =
            super::render::paragraph_spans(&line, &[], cosmic::font::mono().family, FONT_SIZE);
        let layout =
            super::render::layout_preformatted(&spans, FONT_SIZE, text_width - padding.x());
        assert!(
            (layout.min_bounds().height + padding.y() - text_height).abs() < 0.5,
            "{viewport:?}: the code block is {text_height:.1}px high, its layout {:.1}px",
            layout.min_bounds().height + padding.y()
        );
        assert!(
            layout.min_bounds().width + padding.x() <= text_width,
            "{viewport:?}: lines {:.1}px wide overflow the {text_width:.1}px text column",
            layout.min_bounds().width + padding.x()
        );
    }
}

/// Golden snapshots of a code block split across pages: it must fill each
/// page down to the bottom margin.
#[tokio::test]
#[ignore = "golden snapshot differs by ~234 px on Linux CI (font rendering); regenerate the goldens on Linux, then remove this attribute"]
async fn paginated_preformatted_goldens() {
    let (mut viewer, ..) = paginated_viewer(code_chapter(120)).await;
    let _guard = crate::test_support::cosmic_tk_lock();
    cosmic_golden::init();
    let viewport = VIEWPORTS[0];
    let total = paginate(&mut viewer, viewport);
    let (width, height) = (viewport.0 as u32, viewport.1 as u32);
    for (suffix, page) in [("first", 0), ("middle", total / 2)] {
        viewer.current_page = page;
        cosmic_golden::assert_snapshot!(
            &format!("paginated_preformatted_{suffix}"),
            viewer.view_content_paginated(),
            width,
            height
        );
    }
}

/// A paragraph followed by a `tag` list (`ul` or `ol`, with `attributes`) of
/// 60 short items and 5 items longer than a page, spread between them.
fn list_chapter(tag: &str, attributes: &str) -> String {
    let items: String = (0..65)
        .map(|i| {
            let words = if i % 13 == 12 { 400 } else { 3 + i % 5 };
            format!("<li>{}</li>", prose(i, words))
        })
        .collect();
    format!("<p>{}</p><{tag}{attributes}>{items}</{tag}>", prose(0, 60))
}

#[tokio::test]
async fn list_pages_fill_without_overflowing() {
    for tag in ["ul", "ol"] {
        let (mut viewer, ..) = paginated_viewer(list_chapter(tag, "")).await;
        let _guard = crate::test_support::cosmic_tk_lock();
        cosmic_golden::init();
        for viewport in VIEWPORTS {
            let total = paginate(&mut viewer, viewport);
            assert!(
                total > 2,
                "{tag} {viewport:?}: expected several pages, got {total}"
            );
            assert_pages_fill(&mut viewer, viewport, tag);

            let layout = &viewer.pagination_cache[&viewer.active_chapter];
            let cuts: Vec<_> = layout.pages.iter().filter_map(|p| p.end_cut).collect();
            assert!(
                cuts.iter().any(|cut| cut.char_offset == 0),
                "{tag} {viewport:?}: expected a page break between items, got {cuts:?}"
            );
            assert!(
                cuts.iter().any(|cut| cut.char_offset > 0),
                "{tag} {viewport:?}: expected a page break inside an item, got {cuts:?}"
            );
        }
    }
}

/// Golden snapshots of an ordered list split across pages: it must fill each
/// page down to the bottom margin.
#[tokio::test]
async fn paginated_list_goldens() {
    let (mut viewer, ..) = paginated_viewer(list_chapter("ol", "")).await;
    let _guard = crate::test_support::cosmic_tk_lock();
    cosmic_golden::init();
    let viewport = VIEWPORTS[0];
    let total = paginate(&mut viewer, viewport);
    let (width, height) = (viewport.0 as u32, viewport.1 as u32);
    for (suffix, page) in [("first", 0), ("middle", total / 2)] {
        viewer.current_page = page;
        cosmic_golden::assert_snapshot!(
            &format!("paginated_list_{suffix}"),
            viewer.view_content_paginated(),
            width,
            height
        );
    }
}

/// A page starting between the items of an ordered list numbers its first
/// item as the list does; a page continuing an item draws no number.
#[tokio::test]
async fn ordered_list_numbering_continues_on_the_next_page() {
    const START: u32 = 3;
    let (mut viewer, ..) =
        paginated_viewer(list_chapter("ol", &format!(" start=\"{START}\""))).await;
    let _guard = crate::test_support::cosmic_tk_lock();
    cosmic_golden::init();
    for viewport in VIEWPORTS {
        paginate(&mut viewer, viewport);
        let chapter = &viewer.chapters[viewer.active_chapter];
        let layout = &viewer.pagination_cache[&viewer.active_chapter];
        let first_part = |range: &super::PageRange| {
            let ContentBlock::OrderedList { start, items } = &chapter.blocks()[range.start] else {
                panic!("{viewport:?}: the page starts inside the list");
            };
            let to = if range.end - 1 == range.start {
                range.end_cut
            } else {
                None
            };
            let part = super::render::list_item_parts(items, Some(*start), range.start_cut, to)
                .into_iter()
                .next()
                .expect("the page shows an item");
            let spans = super::render::list_item_part_spans(
                &items[part.index],
                &part,
                font::Family::Serif,
                FONT_SIZE,
            );
            (part.index, spans[0].text.to_string())
        };

        let between = layout
            .pages
            .iter()
            .find(|p| p.start_cut.child > 0 && p.start_cut.char_offset == 0)
            .expect("a page starts between items");
        let (index, first_text) = first_part(between);
        assert_eq!(
            first_text,
            format!("  {}. ", index + START as usize),
            "{viewport:?}: first item on the page"
        );

        let inside = layout
            .pages
            .iter()
            .find(|p| p.start_cut.char_offset > 0)
            .expect("a page starts inside an item");
        let ContentBlock::OrderedList { items, .. } = &chapter.blocks()[inside.start] else {
            panic!("{viewport:?}: the page starts inside the list");
        };
        let (index, first_text) = first_part(inside);
        let item = &items[index].text;
        let rest = &item[super::char_offset_to_byte(item, inside.start_cut.char_offset)..];
        assert!(
            !first_text.is_empty() && rest.starts_with(&first_text),
            "{viewport:?}: the continued item draws no number, but starts with {first_text:?}"
        );
    }
}

/// A paragraph followed by a block quote of paragraphs, mostly short, every
/// tenth one longer than a page.
fn block_quote_chapter() -> String {
    let paragraphs: String = (0..30)
        .map(|i| {
            let words = if i % 10 == 9 { 600 } else { 20 + i * 7 % 50 };
            format!("<p>{}</p>", prose(i, words))
        })
        .collect();
    format!(
        "<p>{}</p><blockquote>{paragraphs}</blockquote>",
        prose(0, 60)
    )
}

#[tokio::test]
async fn block_quote_pages_fill_without_overflowing() {
    let (mut viewer, ..) = paginated_viewer(block_quote_chapter()).await;
    let _guard = crate::test_support::cosmic_tk_lock();
    cosmic_golden::init();
    for viewport in VIEWPORTS {
        let total = paginate(&mut viewer, viewport);
        assert!(
            total > 2,
            "{viewport:?}: expected several pages, got {total}"
        );
        assert_pages_fill(&mut viewer, viewport, "block quote");

        let chapter = &viewer.chapters[viewer.active_chapter];
        let layout = &viewer.pagination_cache[&viewer.active_chapter];
        let cuts: Vec<_> = layout
            .pages
            .iter()
            .filter(|p| matches!(chapter.blocks()[p.end - 1], ContentBlock::BlockQuote { .. }))
            .filter_map(|p| p.end_cut)
            .collect();
        assert!(
            cuts.iter().any(|cut| cut.char_offset > 0),
            "{viewport:?}: expected a page break inside a quoted paragraph, got {cuts:?}"
        );
        assert_shows_all_text(&viewer, viewport);
    }
}

/// A paragraph referencing a footnote of several paragraphs, the first one
/// longer than a page.
fn footnote_chapter() -> String {
    let paragraphs: String = (0..8)
        .map(|i| {
            let words = if i == 0 { 900 } else { 40 + i * 11 % 60 };
            format!("<p>{}</p>", prose(i, words))
        })
        .collect();
    format!(
        "<p>{}<sup><a href=\"#fn1\">1</a></sup>.</p>\
         <aside epub:type=\"footnote\" id=\"fn1\">{paragraphs}</aside>",
        prose(0, 60)
    )
}

/// The text of the spans drawing paragraph `child` of footnote `footnote`
/// from character `from` on.
fn footnote_paragraph_text(footnote: &ContentBlock, child: usize, from: usize) -> Vec<String> {
    let ContentBlock::Footnote { id, blocks } = footnote else {
        panic!("expected a footnote, got {footnote:?}");
    };
    let ContentBlock::Paragraph { text, spans, .. } = &blocks[child] else {
        panic!("expected a paragraph, got {:?}", blocks[child]);
    };
    super::render::footnote_paragraph_spans(
        id,
        child,
        text,
        spans,
        from,
        usize::MAX,
        font::Family::Serif,
        FONT_SIZE,
    )
    .into_iter()
    .map(|span| span.text.to_string())
    .collect()
}

#[tokio::test]
async fn footnote_continues_with_indent_not_label() {
    let (mut viewer, ..) = paginated_viewer(footnote_chapter()).await;
    let _guard = crate::test_support::cosmic_tk_lock();
    cosmic_golden::init();
    for viewport in VIEWPORTS {
        let total = paginate(&mut viewer, viewport);
        assert!(
            total > 2,
            "{viewport:?}: expected several pages, got {total}"
        );
        assert_pages_fill(&mut viewer, viewport, "footnote");
        assert_shows_all_text(&viewer, viewport);

        let chapter = &viewer.chapters[viewer.active_chapter];
        let footnote = &chapter.blocks()[1];
        assert_eq!(
            footnote_paragraph_text(footnote, 0, 0)[0],
            "  1.\u{a0}",
            "{viewport:?}: the footnote starts with its label"
        );

        let layout = &viewer.pagination_cache[&viewer.active_chapter];
        let continued = layout
            .pages
            .iter()
            .find(|p| p.start == 1 && p.start_cut.child == 0 && p.start_cut.char_offset > 0)
            .expect("a page continues the footnote's first paragraph");
        let shown = footnote_paragraph_text(footnote, 0, continued.start_cut.char_offset);
        assert_eq!(
            shown[0],
            "\u{a0}".repeat(5),
            "{viewport:?}: the continued footnote starts with the indent"
        );
        let whole = footnote_paragraph_text(footnote, 0, 0).concat();
        assert!(
            whole.ends_with(&shown[1..].concat()),
            "{viewport:?}: the continued footnote shows {:?}, expected the rest of its \
             paragraph",
            shown[1]
        );
    }
}

/// A footnote holding a block quote that holds lists, each child shorter
/// than a page, the block quote longer than one.
fn nested_container_chapter() -> String {
    let quoted: String = (0..6)
        .map(|i| {
            if i % 2 == 1 {
                let items: String = (0..8)
                    .map(|j| format!("<li>{}</li>", prose(i * 10 + j, 6 + j % 4 * 4)))
                    .collect();
                format!("<ul>{items}</ul>")
            } else {
                format!("<p>{}</p>", prose(i, 50))
            }
        })
        .collect();
    format!(
        "<p>{}<sup><a href=\"#fn1\">1</a></sup>.</p>\
         <aside epub:type=\"footnote\" id=\"fn1\"><p>{}</p>\
         <blockquote>{quoted}</blockquote><p>{}</p></aside>",
        prose(0, 200),
        prose(1, 80),
        prose(2, 80)
    )
}

#[tokio::test]
async fn nested_containers_split_without_losing_text() {
    let (mut viewer, ..) = paginated_viewer(nested_container_chapter()).await;
    let _guard = crate::test_support::cosmic_tk_lock();
    cosmic_golden::init();
    let chapter = &viewer.chapters[viewer.active_chapter];
    let ContentBlock::Footnote { blocks, .. } = &chapter.blocks()[1] else {
        panic!("the second block is the footnote");
    };
    let ContentBlock::BlockQuote { children } = &blocks[1] else {
        panic!("the footnote holds a block quote");
    };
    assert!(
        matches!(children[1], ContentBlock::UnorderedList { .. }),
        "the block quote holds a list"
    );
    for viewport in VIEWPORTS {
        let total = paginate(&mut viewer, viewport);
        assert!(
            total > 2,
            "{viewport:?}: expected several pages, got {total}"
        );
        page_text_heights(&mut viewer, viewport);
        let layout = &viewer.pagination_cache[&viewer.active_chapter];
        assert!(
            layout
                .pages
                .iter()
                .any(|p| p.start == 1 && p.start_cut.child == 1 && p.start_cut.char_offset > 0),
            "{viewport:?}: expected a page to start inside the block quote, got {:?}",
            layout.pages
        );
        assert_shows_all_text(&viewer, viewport);
    }
}

/// A block quote holding a block quote whose first paragraph is taller than a
/// page, then a table taller than a page, which cannot be split.
fn oversized_children_chapter() -> String {
    let rows: String = (0..60)
        .map(|i| format!("<tr><td>{i}</td><td>{}</td></tr>", prose(i, 2)))
        .collect();
    format!(
        "<p>{}</p><blockquote><p>{}</p><blockquote><p>{}</p><p>{}</p><p>{}</p>\
         </blockquote><table>{rows}</table><p>{}</p></blockquote>",
        prose(0, 60),
        prose(1, 20),
        prose(2, 900),
        prose(3, 40),
        prose(4, 40),
        prose(5, 60)
    )
}

#[tokio::test]
async fn oversized_container_children_are_placed_alone() {
    let (mut viewer, ..) = paginated_viewer(oversized_children_chapter()).await;
    let _guard = crate::test_support::cosmic_tk_lock();
    cosmic_golden::init();
    let cut = |child, char_offset| super::BlockCut { child, char_offset };
    for viewport in VIEWPORTS {
        paginate(&mut viewer, viewport);
        assert_shows_all_text(&viewer, viewport);
        let layout = viewer.pagination_cache[&viewer.active_chapter].clone();
        let page_of = |start_cut| {
            layout
                .pages
                .iter()
                .position(|p| p.start == 1 && p.start_cut == start_cut)
                .unwrap_or_else(|| {
                    panic!(
                        "{viewport:?}: no page starts at {start_cut:?}: {:?}",
                        layout.pages
                    )
                })
        };
        // The nested block quote's first paragraph, then the table, each
        // overflow a page on their own.
        let alone = [
            (page_of(cut(1, 0)), cut(1, 1)),
            (page_of(cut(2, 0)), cut(3, 0)),
        ];
        for (page, end_cut) in alone {
            let range = &layout.pages[page];
            assert_eq!(
                (range.end, range.end_cut),
                (2, Some(end_cut)),
                "{viewport:?} page {page}: expected only the oversized child"
            );
        }
        let mut page = 0;
        while page < layout.pages.len() {
            let fits = page_fits(&mut viewer, page, viewport);
            for (side, fit) in fits.iter().enumerate() {
                let shown = page + side;
                assert!(
                    fit.gap() >= -0.5 || alone.iter().any(|(page, _)| *page == shown),
                    "{viewport:?} page {shown}: text overflows by {:.1}px",
                    -fit.gap()
                );
            }
            page += fits.len();
        }
    }
}

/// The text of `block` shown from `from` up to `to` (`None`: to its end).
fn part_text(block: &ContentBlock, from: super::BlockCut, to: Option<super::BlockCut>) -> String {
    let slice = |text: &str, start: usize, end: Option<usize>| {
        let count = text.chars().count();
        let end = end.unwrap_or(count).min(count);
        text[char_offset_to_byte(text, start.min(end))..char_offset_to_byte(text, end)].to_string()
    };
    if let Some((items, start)) = super::list_parts(block) {
        return super::render::list_item_parts(items, start, from, to)
            .iter()
            .map(|part| {
                let end = (part.end != usize::MAX).then_some(part.end);
                slice(&items[part.index].text, part.start, end)
            })
            .collect();
    }
    if let Some(children) = super::render::container_children(block) {
        let end = to.map_or(children.len(), |to| {
            to.child + usize::from(to.char_offset > 0)
        });
        return (from.child..end.min(children.len()))
            .map(|index| {
                let child = &children[index];
                let child_from = if index == from.child {
                    super::container_child_cut(child, from.char_offset)
                } else {
                    super::BlockCut::default()
                };
                let child_to = to
                    .filter(|to| to.child == index)
                    .map(|to| super::container_child_cut(child, to.char_offset));
                part_text(child, child_from, child_to)
            })
            .collect();
    }
    match block {
        ContentBlock::Paragraph { text, .. } | ContentBlock::Preformatted { text, .. } => {
            slice(text, from.char_offset, to.map(|to| to.char_offset))
        }
        // The header rows a continued table repeats are left out: they are
        // the only text shown twice.
        ContentBlock::Table { rows } => super::render::table_part_rows(rows, from, to)
            .iter()
            .filter(|part| part.index >= from.child)
            .flat_map(|part| rows[part.index].iter().map(|cell| cell.text.clone()))
            .collect(),
        _ => String::new(),
    }
}

/// Check that the pages of the active chapter together show all of its text,
/// each part once.
fn assert_shows_all_text(viewer: &EpubViewer, viewport: (f32, f32)) {
    let chapter = &viewer.chapters[viewer.active_chapter];
    let layout = &viewer.pagination_cache[&viewer.active_chapter];
    let whole: String = chapter
        .blocks()
        .iter()
        .map(|block| part_text(block, super::BlockCut::default(), None))
        .collect();
    let paged: String = layout
        .pages
        .iter()
        .flat_map(|page| {
            (page.start..page.end).map(move |index| {
                let from = if index == page.start {
                    page.start_cut
                } else {
                    super::BlockCut::default()
                };
                let to = if index + 1 == page.end {
                    page.end_cut
                } else {
                    None
                };
                part_text(&chapter.blocks()[index], from, to)
            })
        })
        .collect();
    assert!(
        paged == whole,
        "{viewport:?}: the pages show {} characters of the chapter's {}",
        paged.chars().count(),
        whole.chars().count()
    );
}

/// Golden snapshots of a block quote and of a footnote split across pages:
/// they must fill each page down to the bottom margin.
#[tokio::test]
async fn paginated_container_goldens() {
    for (name, body) in [
        ("paginated_blockquote", block_quote_chapter()),
        ("paginated_footnote", footnote_chapter()),
    ] {
        let (mut viewer, ..) = paginated_viewer(body).await;
        let _guard = crate::test_support::cosmic_tk_lock();
        cosmic_golden::init();
        let viewport = VIEWPORTS[0];
        let total = paginate(&mut viewer, viewport);
        let (width, height) = (viewport.0 as u32, viewport.1 as u32);
        viewer.current_page = total / 2;
        cosmic_golden::assert_snapshot!(
            &format!("{name}_middle"),
            viewer.view_content_paginated(),
            width,
            height
        );
    }
}

/// A paragraph, then a table with a header row whose second cell has
/// `header_words` words and 80 body rows, then a paragraph. Every 17th body
/// row has a description of `long_row_words` words.
fn table_chapter(header_words: usize, long_row_words: usize) -> String {
    labelled_table_chapter("td", header_words, long_row_words)
}

/// [`table_chapter`] with the number starting each body row in a
/// `label_tag` cell: `td`, or `th` for a row label.
fn labelled_table_chapter(label_tag: &str, header_words: usize, long_row_words: usize) -> String {
    let rows: String = (0..80)
        .map(|i| {
            let words = if i % 17 == 16 {
                long_row_words
            } else {
                2 + i % 4
            };
            format!(
                "<tr><{label_tag}>{i}</{label_tag}><td>{}</td></tr>",
                prose(i, words)
            )
        })
        .collect();
    format!(
        "<p>{}</p><table><tr><th>Number</th><th>{}</th></tr>{rows}</table><p>{}</p>",
        prose(0, 60),
        prose(99, header_words),
        prose(1, 60)
    )
}

/// Index of the table in the chapters [`table_chapter`] builds.
const TABLE: usize = 1;

/// The rows of the active chapter's table.
fn table_rows(viewer: &EpubViewer) -> Vec<Vec<epub::TableCell>> {
    match &viewer.chapters[viewer.active_chapter].blocks()[TABLE] {
        ContentBlock::Table { rows } => rows.clone(),
        other => panic!("expected a table, got {other:?}"),
    }
}

/// Height of row `index` of table `rows` below a light divider, as a page
/// of the active chapter draws it.
fn table_row_height(viewer: &EpubViewer, rows: &[Vec<epub::TableCell>], index: usize) -> f32 {
    let width = viewer.pagination_cache[&viewer.active_chapter].page_width;
    viewer.measurer.borrow_mut().height(
        super::render::render_table_row(
            &rows[index],
            super::render::TableDivider::Light,
            FONT_SIZE,
            font::Family::Serif,
        ),
        width,
    )
}

/// The part of the table page `page` of the active chapter shows: where it
/// starts and ends; `None` when the page shows none of it.
fn table_part(
    viewer: &EpubViewer,
    page: usize,
) -> Option<(super::BlockCut, Option<super::BlockCut>)> {
    let range = &viewer.pagination_cache[&viewer.active_chapter].pages[page];
    (range.start..range.end).contains(&TABLE).then(|| {
        let from = if range.start == TABLE {
            range.start_cut
        } else {
            super::BlockCut::default()
        };
        let to = if range.end == TABLE + 1 {
            range.end_cut
        } else {
            None
        };
        (from, to)
    })
}

#[tokio::test]
async fn table_pages_fill_without_overflowing() {
    let (mut viewer, ..) = paginated_viewer(table_chapter(1, 30)).await;
    let _guard = crate::test_support::cosmic_tk_lock();
    cosmic_golden::init();
    let spacing = cosmic::theme::active().cosmic().spacing.space_xxs as f32;
    for viewport in VIEWPORTS {
        let total = paginate(&mut viewer, viewport);
        assert!(
            total > 2,
            "{viewport:?}: expected several pages, got {total}"
        );
        assert_shows_all_text(&viewer, viewport);
        let rows = table_rows(&viewer);
        let pages = viewer.pagination_cache[&viewer.active_chapter]
            .pages
            .clone();
        assert!(
            pages
                .iter()
                .any(|p| p.end == TABLE + 1 && p.end_cut.is_some()),
            "{viewport:?}: expected a page break inside the table, got {pages:?}"
        );
        // The table's first part: its header row and first body row.
        let table_start = table_row_height(&viewer, &rows, 0) + table_row_height(&viewer, &rows, 1);
        let mut page = 0;
        while page < total {
            let fits = page_fits(&mut viewer, page, viewport);
            for (side, fit) in fits.iter().enumerate() {
                let shown = page + side;
                let gap = fit.gap();
                assert!(
                    gap >= -0.5,
                    "{viewport:?} page {shown}: text overflows by {:.1}px",
                    -gap
                );
                if shown + 1 == total {
                    continue;
                }
                // The next unit that did not fit: the table row the page is
                // cut before, or a line or the table's first part.
                let bound = match pages[shown].end_cut {
                    Some(cut) if pages[shown].end == TABLE + 1 => {
                        table_row_height(&viewer, &rows, cut.child)
                    }
                    _ => LINE_ADVANCE.max(table_start) + spacing,
                };
                assert!(
                    gap < bound,
                    "{viewport:?} page {shown}: bottom gap {gap:.1}px leaves room for \
                     {bound:.1}px"
                );
            }
            page += fits.len();
        }
    }
}

#[tokio::test]
async fn table_header_repeats_on_continuation_pages() {
    let (mut viewer, ..) = paginated_viewer(table_chapter(1, 30)).await;
    let _guard = crate::test_support::cosmic_tk_lock();
    cosmic_golden::init();
    for viewport in VIEWPORTS {
        let total = paginate(&mut viewer, viewport);
        let rows = table_rows(&viewer);
        assert!(
            rows[0].iter().all(|cell| cell.is_header),
            "the header row's cells are drawn bold"
        );
        let mut continued = 0;
        for page in 1..total {
            let Some((from, to)) = table_part(&viewer, page) else {
                continue;
            };
            if from == super::BlockCut::default() {
                continue;
            }
            continued += 1;
            let parts = super::render::table_part_rows(&rows, from, to);
            assert_eq!(
                parts[..2],
                [
                    super::render::TableRowPart {
                        index: 0,
                        divider: super::render::TableDivider::Light,
                    },
                    super::render::TableRowPart {
                        index: from.child,
                        divider: super::render::TableDivider::Heavy,
                    },
                ],
                "{viewport:?} page {page}: expected the header row, then a heavy divider"
            );
        }
        assert!(
            continued >= 2,
            "{viewport:?}: expected the table to continue on several pages"
        );
    }
}

#[tokio::test]
async fn table_with_row_labels_repeats_its_header_row() {
    let (mut viewer, ..) = paginated_viewer(labelled_table_chapter("th", 1, 30)).await;
    let _guard = crate::test_support::cosmic_tk_lock();
    cosmic_golden::init();
    for viewport in VIEWPORTS {
        let total = paginate(&mut viewer, viewport);
        assert_shows_all_text(&viewer, viewport);
        let rows = table_rows(&viewer);
        assert!(
            rows[1..]
                .iter()
                .all(|row| row[0].is_header && !row[1].is_header),
            "every body row starts with a row label"
        );
        let mut continued = 0;
        for page in 1..total {
            let Some((from, to)) = table_part(&viewer, page) else {
                continue;
            };
            if from == super::BlockCut::default() {
                continue;
            }
            continued += 1;
            let shown: Vec<usize> = super::render::table_part_rows(&rows, from, to)
                .iter()
                .map(|part| part.index)
                .take(2)
                .collect();
            assert_eq!(
                shown,
                [0, from.child],
                "{viewport:?} page {page}: expected the header row, then the page's rows"
            );
        }
        assert!(
            continued >= 2,
            "{viewport:?}: expected the table to continue on several pages"
        );
    }
}

/// A paragraph, then a table whose header row starts with an empty corner
/// cell and has 80 body rows, each starting with a `label_tag` cell, then a
/// paragraph.
fn corner_table_chapter(label_tag: &str) -> String {
    let rows: String = (0..80)
        .map(|i| {
            format!(
                "<tr><{label_tag}>{i}</{label_tag}><td>{}</td></tr>",
                prose(i, 2 + i % 4)
            )
        })
        .collect();
    format!(
        "<p>{}</p><table><tr><td></td><th>Description</th></tr>{rows}</table><p>{}</p>",
        prose(0, 60),
        prose(1, 60)
    )
}

/// Assert that every continuation page of the table in `chapter` repeats its
/// first row.
async fn assert_header_row_repeats(chapter: String) {
    let (mut viewer, ..) = paginated_viewer(chapter).await;
    let _guard = crate::test_support::cosmic_tk_lock();
    cosmic_golden::init();
    for viewport in VIEWPORTS {
        let total = paginate(&mut viewer, viewport);
        assert_shows_all_text(&viewer, viewport);
        let rows = table_rows(&viewer);
        let mut continued = 0;
        for page in 1..total {
            let Some((from, to)) = table_part(&viewer, page) else {
                continue;
            };
            if from == super::BlockCut::default() {
                continue;
            }
            continued += 1;
            let shown: Vec<usize> = super::render::table_part_rows(&rows, from, to)
                .iter()
                .map(|part| part.index)
                .take(2)
                .collect();
            assert_eq!(
                shown,
                [0, from.child],
                "{viewport:?} page {page}: expected the header row, then the page's rows"
            );
        }
        assert!(
            continued >= 2,
            "{viewport:?}: expected the table to continue on several pages"
        );
    }
}

#[tokio::test]
async fn table_header_row_with_an_empty_corner_cell_repeats() {
    assert_header_row_repeats(corner_table_chapter("td")).await;
}

#[tokio::test]
async fn table_with_a_corner_cell_and_row_labels_repeats_its_header_row() {
    assert_header_row_repeats(corner_table_chapter("th")).await;
}

#[tokio::test]
async fn table_with_a_header_taller_than_a_page_does_not_repeat_it() {
    let (mut viewer, ..) = paginated_viewer(table_chapter(1500, 30)).await;
    let _guard = crate::test_support::cosmic_tk_lock();
    cosmic_golden::init();
    for viewport in VIEWPORTS {
        let total = paginate(&mut viewer, viewport);
        assert_shows_all_text(&viewer, viewport);
        let rows = table_rows(&viewer);
        let mut continued = 0;
        for page in 1..total {
            let Some((from, to)) = table_part(&viewer, page) else {
                continue;
            };
            if from == super::BlockCut::default() {
                continue;
            }
            continued += 1;
            let parts = super::render::table_part_rows(&rows, from, to);
            assert_eq!(
                parts[0].index, from.child,
                "{viewport:?} page {page}: expected no repeated header row"
            );
        }
        assert!(
            continued >= 2,
            "{viewport:?}: expected the table to continue on several pages"
        );
    }
}

#[tokio::test]
async fn table_rows_taller_than_a_page_are_placed_alone() {
    let (mut viewer, ..) = paginated_viewer(table_chapter(1, 1500)).await;
    let _guard = crate::test_support::cosmic_tk_lock();
    cosmic_golden::init();
    for viewport in VIEWPORTS {
        let total = paginate(&mut viewer, viewport);
        assert_shows_all_text(&viewer, viewport);
        // Body row `i` is table row `i + 1`, after the header row.
        let tall: Vec<usize> = (0..80).filter(|i| i % 17 == 16).map(|i| i + 1).collect();
        let mut alone = Vec::new();
        for row in &tall {
            let page = (0..total)
                .find(|&page| {
                    table_part(&viewer, page).is_some_and(|(from, to)| {
                        from.child == *row && to.map(|to| to.child) == Some(row + 1)
                    })
                })
                .unwrap_or_else(|| {
                    panic!(
                        "{viewport:?}: no page holds only row {row}: {:?}",
                        viewer.pagination_cache[&viewer.active_chapter].pages
                    )
                });
            alone.push(page);
        }
        let mut page = 0;
        while page < total {
            let fits = page_fits(&mut viewer, page, viewport);
            for (side, fit) in fits.iter().enumerate() {
                let shown = page + side;
                assert!(
                    fit.gap() >= -0.5 || alone.contains(&shown),
                    "{viewport:?} page {shown}: text overflows by {:.1}px",
                    -fit.gap()
                );
            }
            page += fits.len();
        }
    }
}

/// Golden snapshot of a table continued on a page: the header row repeated
/// above the page's rows, filling the page down to the bottom margin.
#[tokio::test]
async fn paginated_table_goldens() {
    let (mut viewer, ..) = paginated_viewer(table_chapter(1, 30)).await;
    let _guard = crate::test_support::cosmic_tk_lock();
    cosmic_golden::init();
    let viewport = VIEWPORTS[0];
    let total = paginate(&mut viewer, viewport);
    let (width, height) = (viewport.0 as u32, viewport.1 as u32);
    viewer.current_page = total / 2;
    cosmic_golden::assert_snapshot!(
        "paginated_table_middle",
        viewer.view_content_paginated(),
        width,
        height
    );
}

/// A PNG of `width` × `height` pixels.
fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::RgbImage::from_pixel(width, height, image::Rgb([200, 120, 40]))
        .write_to(&mut bytes, image::ImageFormat::Png)
        .expect("encode png");
    bytes.into_inner()
}

/// Paragraphs around an image far taller than any page, between `before` and
/// `after` (markup of a block holding it, or nothing): an SVG drawing when
/// `svg`, a PNG otherwise. The first paragraph references footnote `fn1`.
async fn tall_image_viewer(
    svg: bool,
    before: &str,
    after: &str,
) -> (
    EpubViewer,
    tempfile::NamedTempFile,
    tempfile::TempDir,
    tempfile::TempDir,
) {
    let image = if svg {
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"300\" height=\"3000\" \
         viewBox=\"0 0 300 3000\"><rect width=\"300\" height=\"3000\" fill=\"#c87828\"/></svg>"
    } else {
        "<img src=\"images/tall.png\" alt=\"Tall\"/>"
    };
    paginated_viewer_with_resources(
        format!(
            "<p>{}<sup><a href=\"#fn1\">1</a></sup>.</p>{before}{image}{after}<p>{}</p>",
            prose(0, 300),
            prose(1, 300)
        ),
        vec![("OEBPS/images/tall.png", png(300, 3000), "image/png")],
    )
    .await
}

/// Whether `block` is an image (raster with data, or SVG) or holds one.
fn holds_image(block: &ContentBlock) -> bool {
    match block {
        ContentBlock::Image { data, .. } => !data.is_empty(),
        ContentBlock::Svg { .. } => true,
        _ => super::render::container_children(block)
            .is_some_and(|children| children.iter().any(holds_image)),
    }
}

/// Index of the first block in the active chapter that is an image or holds
/// one.
fn image_block(viewer: &EpubViewer) -> usize {
    viewer.chapters[viewer.active_chapter]
        .blocks()
        .iter()
        .position(holds_image)
        .expect("an image block")
}

#[rstest::rstest]
#[case::raster(false, "", "")]
#[case::svg(true, "", "")]
#[case::block_quote(false, "<blockquote>", "</blockquote>")]
#[case::svg_in_block_quote(true, "<blockquote>", "</blockquote>")]
#[case::figure_with_caption(
    false,
    "<figure>",
    "<figcaption>A caption below a drawing far taller than the page</figcaption></figure>"
)]
#[case::footnote(false, "<aside epub:type=\"footnote\" id=\"fn1\">", "</aside>")]
#[tokio::test]
async fn image_taller_than_the_page_scales_to_fit(
    #[case] svg: bool,
    #[case] before: &str,
    #[case] after: &str,
) {
    let (mut viewer, ..) = tall_image_viewer(svg, before, after).await;
    let _guard = crate::test_support::cosmic_tk_lock();
    cosmic_golden::init();
    let spacing = cosmic::theme::active().cosmic().spacing.space_xxs as f32;
    let image = image_block(&viewer);
    for viewport in VIEWPORTS {
        let total = paginate(&mut viewer, viewport);
        assert_shows_all_text(&viewer, viewport);
        let layout = viewer.pagination_cache[&viewer.active_chapter].clone();
        assert!(
            layout.pages.iter().all(|page| page.start < page.end),
            "{viewport:?}: expected no empty page, got {:?}",
            layout.pages
        );
        let image_page = layout
            .pages
            .iter()
            .position(|page| page.start <= image && image < page.end)
            .expect("a page showing the image");
        let range = &layout.pages[image_page];
        assert_eq!(
            (range.start, range.start_cut, range.end),
            (image, super::BlockCut::default(), image + 1),
            "{viewport:?}: expected the image alone on its page, got {:?}",
            layout.pages
        );
        let mut page = 0;
        while page < total {
            let fits = page_fits(&mut viewer, page, viewport);
            for (side, fit) in fits.iter().enumerate() {
                let shown = page + side;
                let gap = fit.gap();
                assert!(
                    gap >= -0.5,
                    "{viewport:?} page {shown}: text overflows by {:.1}px",
                    -gap
                );
                if shown + 1 == total {
                    continue;
                }
                // The next unit that did not fit: the image (as tall as the
                // page), or a line.
                let bound = if shown + 1 == image_page {
                    layout.page_height + spacing
                } else if shown == image_page {
                    LINE_ADVANCE
                } else {
                    LINE_ADVANCE + spacing
                };
                assert!(
                    gap < bound,
                    "{viewport:?} page {shown}: bottom gap {gap:.1}px leaves room for \
                     {bound:.1}px"
                );
            }
            page += fits.len();
        }
    }
}

/// Golden snapshot of an image taller than the page, scaled down to fill
/// its page exactly.
#[tokio::test]
async fn paginated_image_page_golden() {
    let (mut viewer, ..) = tall_image_viewer(false, "", "").await;
    let _guard = crate::test_support::cosmic_tk_lock();
    cosmic_golden::init();
    let viewport = VIEWPORTS[0];
    paginate(&mut viewer, viewport);
    let image = image_block(&viewer);
    let (width, height) = (viewport.0 as u32, viewport.1 as u32);
    viewer.current_page = viewer.pagination_cache[&viewer.active_chapter]
        .pages
        .iter()
        .position(|page| page.start == image)
        .expect("a page starting at the image");
    cosmic_golden::assert_snapshot!(
        "paginated_image_page",
        viewer.view_content_paginated(),
        width,
        height
    );
}

/// Sections of a heading (a part heading and a section heading for every
/// third) and a paragraph of varying length, so that headings come near the
/// bottom of pages. With `ids`, every heading and paragraph has an id, which
/// puts an anchor before it.
fn heading_chapter(ids: bool) -> String {
    let id = |name: &str, section: usize| {
        if ids {
            format!(" id=\"{name}-{section}\"")
        } else {
            String::new()
        }
    };
    (0..24)
        .map(|section| {
            let part = if section % 3 == 0 {
                format!("<h1{}>Part {section}</h1>", id("part", section))
            } else {
                String::new()
            };
            format!(
                "{part}<h2{}>Section {section}</h2><p{}>{}</p>",
                id("section", section),
                id("text", section),
                prose(section, 20 + section * 37 % 110)
            )
        })
        .collect()
}

#[rstest::rstest]
#[case::plain(false)]
#[case::with_ids(true)]
#[tokio::test]
async fn heading_is_not_left_alone_at_the_bottom_of_a_page(#[case] ids: bool) {
    let (mut viewer, ..) = paginated_viewer(heading_chapter(ids)).await;
    let _guard = crate::test_support::cosmic_tk_lock();
    cosmic_golden::init();
    for viewport in VIEWPORTS {
        let total = paginate(&mut viewer, viewport);
        assert_shows_all_text(&viewer, viewport);
        let chapter = &viewer.chapters[viewer.active_chapter];
        let layout = &viewer.pagination_cache[&viewer.active_chapter];
        assert_eq!(
            chapter.blocks().iter().any(super::is_anchor),
            ids,
            "anchors only with ids"
        );
        for (page, range) in layout.pages.iter().enumerate().take(total - 1) {
            let last_shown = chapter.blocks()[range.start..range.end]
                .iter()
                .rev()
                .find(|block| !super::is_anchor(block))
                .expect("a page shows a block");
            assert!(
                !super::is_heading(last_shown),
                "{viewport:?} page {page} ends with a heading: {last_shown:?}"
            );
        }
        // A link to an anchor opens the page showing the block after it.
        let page_of = |block: usize| {
            layout
                .pages
                .iter()
                .position(|page| page.start <= block && block < page.end)
                .expect("every block is on a page")
        };
        for (anchor, block) in chapter.blocks().iter().enumerate() {
            if let ContentBlock::Anchor { id } = block {
                assert_eq!(
                    page_of(anchor),
                    page_of(anchor + 1),
                    "{viewport:?}: anchor #{id} is not on the page showing its block"
                );
            }
        }
        assert_pages_fill(&mut viewer, viewport, "headings");
    }
}

#[tokio::test]
async fn heading_before_a_block_as_tall_as_a_page_stays_in_place() {
    let (mut viewer, ..) = tall_image_viewer(false, "<h2>A drawing</h2>", "").await;
    let _guard = crate::test_support::cosmic_tk_lock();
    cosmic_golden::init();
    let image = image_block(&viewer);
    for viewport in VIEWPORTS {
        paginate(&mut viewer, viewport);
        assert_shows_all_text(&viewer, viewport);
        let layout = &viewer.pagination_cache[&viewer.active_chapter];
        assert!(
            layout
                .pages
                .iter()
                .any(|page| (page.start, page.start_cut, page.end)
                    == (image, super::BlockCut::default(), image + 1)),
            "{viewport:?}: expected the image alone on its page, got {:?}",
            layout.pages
        );
        assert!(
            layout
                .pages
                .iter()
                .any(|page| page.start < image - 1 && page.end == image),
            "{viewport:?}: expected the heading below the paragraph before it, got {:?}",
            layout.pages
        );
    }
}

/// Golden snapshot of a part and a section heading moved to the next page
/// with the text after them, leaving the page before them short.
#[tokio::test]
async fn paginated_heading_kept_with_text_golden() {
    let (mut viewer, ..) = paginated_viewer(heading_chapter(false)).await;
    let _guard = crate::test_support::cosmic_tk_lock();
    cosmic_golden::init();
    let viewport = VIEWPORTS[2];
    paginate(&mut viewer, viewport);
    let spacing = cosmic::theme::active().cosmic().spacing.space_xxs as f32;
    let next = (1..viewer.total_pages())
        .find(|&page| !leading_headings(&viewer, page).is_empty())
        .expect("a page starting with a heading");
    let fits = page_fits(&mut viewer, next - 1, viewport);
    assert!(
        fits.len() == 2 && fits[0].gap() > LINE_ADVANCE + spacing,
        "expected the short page and the page starting with the headings side by side"
    );
    let (width, height) = (viewport.0 as u32, viewport.1 as u32);
    cosmic_golden::assert_snapshot!(
        "paginated_heading_kept_with_text",
        viewer.view_content_paginated(),
        width,
        height
    );
}
