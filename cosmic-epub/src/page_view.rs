//! Responsive single-page and two-page reading surface for a chapter.

use std::cell::Cell;
use std::cell::RefCell;

use cosmic::Element;
use cosmic::cosmic_theme;
use cosmic::iced::Length;
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::alignment::Vertical;
use cosmic::theme;
use cosmic::widget;

use crate::EpubChapter;
use crate::measure::BlockMeasurer;
use crate::pagination::BlockCut;
use crate::pagination::PageAnchor;
use crate::pagination::PaginationLayout;
use crate::pagination::paginate_blocks;
use crate::pagination::render_block_part;
use crate::render::BlockHighlight;
use crate::render::RenderContext;
use crate::render::RenderMessage;

const ICON_SIZE: u16 = 16;

/// Whether to display two pages side by side in paginated mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DualPageMode {
    #[default]
    Auto,
    Off,
    On,
}

impl DualPageMode {
    pub fn is_dual(self, viewport_width: f32) -> bool {
        match self {
            Self::On => true,
            Self::Off => false,
            Self::Auto => viewport_width > 1200.0,
        }
    }

    pub fn step(self, viewport_width: f32) -> usize {
        if self.is_dual(viewport_width) { 2 } else { 1 }
    }
}

#[derive(Clone, Debug)]
pub enum PageViewMessage {
    Block(RenderMessage),
    PreviousPage,
    NextPage,
}

/// A chapter and the current state used to draw its page or spread.
pub struct PageView<'a> {
    pub chapter: &'a EpubChapter,
    pub context: RenderContext<'a>,
    pub layout: Option<&'a PaginationLayout>,
    pub current_page: usize,
    pub pending_anchor: Option<PageAnchor>,
    pub page_margin: f32,
    pub spread: DualPageMode,
    pub viewport_size: &'a Cell<(f32, f32)>,
    pub page_area: &'a Cell<(f32, f32)>,
    pub measurer: &'a RefCell<BlockMeasurer>,
}

impl<'a> PageView<'a> {
    pub fn view(
        self,
        highlight_at: impl Fn(usize) -> BlockHighlight + Copy + 'a,
    ) -> Element<'a, PageViewMessage> {
        let Self {
            chapter,
            context,
            layout,
            current_page,
            pending_anchor,
            page_margin,
            spread,
            viewport_size,
            page_area,
            measurer,
        } = self;
        let cosmic_theme::Spacing {
            space_s, space_xxs, ..
        } = theme::active().cosmic().spacing;
        let context_for = move |max_image_height| RenderContext {
            max_image_height,
            ..context
        };
        widget::responsive(move |size| {
            viewport_size.set((size.width, size.height));
            let dual = spread.is_dual(size.width);

            // The pages are laid out inside the exact area left for them, so
            // pagination measures against the space the text really gets.
            let pages = widget::responsive(move |area| {
                page_area.set((area.width, area.height));

                // Use cached layout if available, otherwise compute on-the-fly so
                // the first render after switching to paginated mode isn't empty.
                let (cw, ch) = page_text_size(
                    (area.width, area.height),
                    dual,
                    space_xxs as f32,
                    page_margin,
                );
                let computed_layout;
                let layout = match layout {
                    Some(l)
                        if pending_anchor.is_none()
                            || ((l.page_width - cw).abs() <= 1.0
                                && (l.page_height - ch).abs() <= 1.0) =>
                    {
                        l
                    }
                    _ => {
                        computed_layout = paginate_blocks(
                            chapter.blocks(),
                            ch,
                            cw,
                            space_xxs as f32,
                            &context_for(ch),
                            &mut measurer.borrow_mut(),
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
                let current_page =
                    pending_anchor.map_or(current_page, |anchor| layout.page_at(anchor));
                // Clamp page index to valid range (may be stale before update runs).
                let current_page = current_page.min(total.saturating_sub(1));

                // Build a single page "paper" element from a page index.
                let make_paper = |page_idx: usize| -> Element<'_, PageViewMessage> {
                    let page_range = layout.pages.get(page_idx);

                    let mut column = widget::Column::new().spacing(space_xxs).width(Length::Fill);
                    if let Some(range) = page_range {
                        let block_count = range.end - range.start;
                        for (i, block) in
                            chapter.blocks()[range.start..range.end].iter().enumerate()
                        {
                            let abs_idx = range.start + i;
                            let highlight = highlight_at(abs_idx);
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
                            column = column.push(
                                render_block_part(
                                    &context_for(layout.page_height),
                                    block,
                                    from,
                                    to,
                                    highlight,
                                )
                                .map(PageViewMessage::Block),
                            );
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
            .on_press(PageViewMessage::PreviousPage);

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
            .on_press(PageViewMessage::NextPage);

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

pub fn paper_background(theme: &cosmic::Theme) -> widget::container::Style {
    let c = theme.cosmic().bg_color();
    widget::container::background(cosmic::iced::Color::from_rgba(
        c.color.red,
        c.color.green,
        c.color.blue,
        c.alpha,
    ))
}

/// Height of the page number line under each page in paginated mode.
const PAGE_INDICATOR_HEIGHT: f32 = 17.0;
/// Id of the container holding a page's text, including its margins.
pub fn page_text_id() -> widget::Id {
    widget::Id::new("epub-page-text")
}
/// Widest a page's text column gets, including its margins.
const MAX_PAGE_WIDTH: f32 = 800.0;

/// Width and height of one page's text when the pages fill `area`: one page,
/// or two side by side `gap` apart, each with `margin` on all sides and the
/// page number line below.
pub fn page_text_size(area: (f32, f32), dual: bool, gap: f32, margin: f32) -> (f32, f32) {
    let (width, height) = area;
    let paper_width = if dual { (width - gap) / 2.0 } else { width };
    (
        (paper_width.min(MAX_PAGE_WIDTH) - margin * 2.0).max(1.0),
        (height - PAGE_INDICATOR_HEIGHT - margin * 2.0).max(1.0),
    )
}
