//! State for an embeddable paginated reader. The host owns file selection
//! and reading-position storage.

use std::cell::Cell;
use std::cell::RefCell;
use std::path::Path;
use std::sync::Arc;

use cosmic::Element;
use cosmic::iced::font;
use cosmic::theme;
use cosmic::widget;
use epub::EpubDocument;
use epub::Locator;
use epub::NavEntry;

use crate::EpubChapter;
use crate::load_epub_chapters;
use crate::load_linked_chapter;
use crate::measure::BlockMeasurer;
use crate::page_view::DualPageMode;
use crate::page_view::PageView;
use crate::page_view::PageViewMessage;
use crate::page_view::page_text_size;
use crate::pagination::BlockCut;
use crate::pagination::PageAnchor;
use crate::pagination::PageTurn;
use crate::pagination::PaginationLayout;
use crate::pagination::last_spread;
use crate::pagination::page_turn;
use crate::pagination::paginate_blocks;
use crate::render::BlockHighlight;
use crate::render::RenderContext;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Position {
    pub chapter_href: String,
    pub cfi: String,
    pub block: usize,
    pub cut: BlockCut,
}

/// Loaded book data suitable for handing from a worker to the UI thread.
#[derive(Clone)]
pub struct LoadedBook {
    title: String,
    chapters: Vec<EpubChapter>,
    document: Arc<EpubDocument>,
}

impl std::fmt::Debug for LoadedBook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedBook")
            .field("title", &self.title)
            .finish_non_exhaustive()
    }
}

impl LoadedBook {
    pub fn open(path: &Path) -> Result<Self, epub::EpubError> {
        let (title, chapters, document) = load_epub_chapters(path)?;
        Ok(Self {
            title,
            chapters,
            document: Arc::new(document),
        })
    }
}

pub struct Reader {
    title: String,
    document: Arc<EpubDocument>,
    chapters: Vec<EpubChapter>,
    chapter: usize,
    linear_chapters: usize,
    return_from_link: Option<(usize, PageAnchor)>,
    toc: Vec<NavEntry>,
    anchor: PageAnchor,
    font_size: f32,
    spread: DualPageMode,
    layout: Option<PaginationLayout>,
    viewport: Cell<(f32, f32)>,
    page_area: Cell<(f32, f32)>,
    measurer: RefCell<BlockMeasurer>,
}

impl Reader {
    /// Load a book and its initial position. Call on a blocking thread.
    pub fn open(path: &Path, position: Option<Position>) -> Result<Self, epub::EpubError> {
        Ok(Self::from_loaded(path, LoadedBook::open(path)?, position))
    }

    pub fn from_loaded(path: &Path, book: LoadedBook, position: Option<Position>) -> Self {
        let LoadedBook {
            mut title,
            chapters,
            document,
        } = book;
        if title.is_empty() {
            title = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
        }
        let toc = if document.nav().is_empty() {
            chapters
                .iter()
                .map(|chapter| NavEntry {
                    href: chapter.href.clone(),
                    label: chapter.label.clone(),
                    depth: 0,
                })
                .collect()
        } else {
            document.nav().to_vec()
        };
        let mut reader = Self {
            title,
            document,
            linear_chapters: chapters.len(),
            chapters,
            chapter: 0,
            return_from_link: None,
            toc,
            anchor: PageAnchor::default(),
            font_size: 16.0,
            spread: DualPageMode::Auto,
            layout: None,
            viewport: Cell::default(),
            page_area: Cell::default(),
            measurer: RefCell::default(),
        };
        if let Some(position) = position
            && let Some(locator) = Locator::from_cfi(&position.cfi)
            && reader.open_target(&position.chapter_href)
        {
            let chapter = &reader.chapters[reader.chapter];
            reader.anchor = PageAnchor {
                block: if locator.node_path.is_empty() {
                    position.block.min(chapter.blocks().len().saturating_sub(1))
                } else {
                    chapter.block_index_for_path(&locator.node_path)
                },
                cut: position.cut,
            };
        }
        reader
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn toc(&self) -> &[NavEntry] {
        &self.toc
    }

    pub fn chapter_href(&self) -> Option<&str> {
        self.chapters
            .get(self.chapter)
            .map(|chapter| chapter.href.as_str())
    }

    pub fn position(&self) -> Option<Position> {
        let chapter = self.chapters.get(self.chapter)?;
        Some(Position {
            chapter_href: chapter.href.clone(),
            block: self.anchor.block,
            cfi: chapter
                .locator(self.chapter, self.anchor.block)
                .to_cfi(None),
            cut: self.anchor.cut,
        })
    }

    pub fn font_size(&self) -> f32 {
        self.font_size
    }

    pub fn set_font_size(&mut self, size: f32) {
        self.font_size = size.clamp(12.0, 24.0);
        self.layout = None;
    }

    pub fn spread(&self) -> DualPageMode {
        self.spread
    }

    pub fn set_spread(&mut self, spread: DualPageMode) {
        self.spread = spread;
        self.layout = None;
    }

    pub fn select_nav(&mut self, index: usize) {
        if let Some(entry) = self.toc().get(index) {
            self.open_target(&entry.href.clone());
        }
    }

    pub fn follow_link(&mut self, href: &str) {
        let Some(chapter) = self.chapters.get(self.chapter) else {
            return;
        };
        let resolved = if href.starts_with('#') {
            format!("{}{href}", chapter.href)
        } else {
            epub::content::resolve_href(epub::content::base_dir(&chapter.href), href)
        };
        self.open_target(&resolved);
    }

    fn open_target(&mut self, target: &str) -> bool {
        let (href, fragment) = target.split_once('#').unwrap_or((target, ""));
        let chapter = if let Some(chapter) = self
            .chapters
            .iter()
            .position(|chapter| chapter.href == href)
        {
            chapter
        } else if let Ok(chapter) = load_linked_chapter(&self.document, href) {
            let index = self.chapters.len();
            self.chapters.push(chapter);
            index
        } else {
            return false;
        };
        if chapter < self.linear_chapters {
            self.return_from_link = None;
        } else if self.chapter < self.linear_chapters {
            self.return_from_link = Some((self.chapter, self.anchor));
        }
        self.chapter = chapter;
        let block = self.chapters[chapter]
            .block_for_fragment(fragment)
            .unwrap_or_default();
        self.anchor = PageAnchor {
            block,
            cut: BlockCut::default(),
        };
        self.layout = None;
        true
    }

    fn context(chapter: &EpubChapter, font_size: f32, max_image_height: f32) -> RenderContext<'_> {
        RenderContext {
            font_size,
            copy_code_label: || "Copy code".into(),
            family: font::Family::Serif,
            max_image_height,
            image_handles: chapter.image_handles(),
        }
    }

    fn dimensions(&self) -> Option<(f32, f32, f32)> {
        let area = self.page_area.get();
        if area.0 <= 0.0 || area.1 <= 0.0 {
            return None;
        }
        let gap = theme::active().cosmic().spacing.space_xxs as f32;
        let (width, height) =
            page_text_size(area, self.spread.is_dual(self.viewport.get().0), gap, 16.0);
        Some((width, height, gap))
    }

    fn layout(&mut self) -> Option<&PaginationLayout> {
        let (width, height, gap) = self.dimensions()?;
        let chapter = self.chapters.get(self.chapter)?;
        if !self.layout.as_ref().is_some_and(|layout| {
            (layout.page_width - width).abs() <= 1.0 && (layout.page_height - height).abs() <= 1.0
        }) {
            self.layout = Some(paginate_blocks(
                chapter.blocks(),
                height,
                width,
                gap,
                &Self::context(chapter, self.font_size, height),
                self.measurer.get_mut(),
            ));
        }
        self.layout.as_ref()
    }

    pub fn turn_page(&mut self, forward: bool) {
        let step = self.spread.step(self.viewport.get().0);
        let anchor = self.anchor;
        let chapter = self.chapter;
        let chapter_count = self.linear_chapters;
        let Some(layout) = self.layout() else { return };
        let current = layout.page_at(anchor);
        let turn = page_turn(
            current,
            layout.pages.len(),
            step,
            chapter,
            chapter_count,
            forward,
        );
        if chapter >= chapter_count && !matches!(turn, PageTurn::Page(_)) {
            if let Some((chapter, anchor)) = self.return_from_link.take() {
                self.chapter = chapter;
                self.anchor = anchor;
                self.layout = None;
            }
            return;
        }
        match turn {
            PageTurn::Page(index) => self.anchor = layout.pages[index].anchor(),
            PageTurn::NextChapter => {
                self.chapter += 1;
                self.anchor = PageAnchor::default();
                self.layout = None;
            }
            PageTurn::PreviousChapter => {
                self.chapter -= 1;
                self.layout = None;
                if let Some(layout) = self.layout() {
                    self.anchor = layout.pages[last_spread(layout.pages.len(), step)].anchor();
                }
            }
            PageTurn::Stay => {}
        }
    }

    pub fn view(&self) -> Element<'_, PageViewMessage> {
        let Some(chapter) = self.chapters.get(self.chapter) else {
            return widget::text::body("This EPUB has no readable chapters.").into();
        };
        PageView {
            chapter,
            context: Self::context(chapter, self.font_size, 0.0),
            layout: self.layout.as_ref(),
            current_page: 0,
            pending_anchor: Some(self.anchor),
            page_margin: 16.0,
            spread: self.spread,
            viewport_size: &self.viewport,
            page_area: &self.page_area,
            measurer: &self.measurer,
        }
        .view(|_| BlockHighlight::None)
    }
}

#[cfg(test)]
mod tests {
    use epub::ContentBlock;

    use super::*;

    fn fixture() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../features/fixtures/sample.epub")
    }

    fn book_with_note(path: &Path) -> LoadedBook {
        let mut book = LoadedBook::open(path).expect("load book");
        let chapter = &book.chapters[0];
        book.chapters[0] = EpubChapter::new(
            chapter.label.clone(),
            chapter.href.clone(),
            vec![
                ContentBlock::HorizontalRule,
                ContentBlock::Anchor {
                    id: "reference".into(),
                },
                ContentBlock::Footnote {
                    id: "note".into(),
                    blocks: Vec::new(),
                },
            ],
            vec![vec![1, 0], Vec::new(), vec![1, 1]],
            String::new(),
        );
        book
    }

    #[test]
    fn footnote_and_backlink_preserve_their_targets() {
        let path = fixture();
        let mut reader = Reader::from_loaded(&path, book_with_note(&path), None);
        reader.follow_link("#note");
        assert_eq!(reader.position().unwrap().block, 2);
        reader.follow_link("#reference");
        let position = reader.position().unwrap();
        assert_eq!(position.block, 1);

        let reopened = Reader::from_loaded(&path, book_with_note(&path), Some(position.clone()));
        assert_eq!(reopened.position(), Some(position));
    }

    #[test]
    fn turning_past_linear_spine_stays_in_the_linear_spine_after_following_an_appendix() {
        let path = fixture();
        let mut reader = Reader::open(&path, None).expect("open book");
        reader.chapters.push(EpubChapter::new(
            "Appendix".into(),
            "appendix.xhtml".into(),
            vec![ContentBlock::HorizontalRule],
            vec![vec![1, 0]],
            String::new(),
        ));
        reader.viewport.set((800.0, 600.0));
        reader.page_area.set((600.0, 500.0));

        reader.follow_link("/appendix.xhtml");
        assert_eq!(reader.chapter, 1);
        reader.turn_page(false);
        assert_eq!(reader.chapter, 0);
        reader.turn_page(true);
        assert_eq!(reader.chapter, 0);
    }
}
