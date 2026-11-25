use cosmic::Element;
use cosmic::iced::advanced::graphics::text::Paragraph;
use cosmic::iced::advanced::text::Paragraph as _;
use cosmic::iced::font;
use epub::BlockStyle;
use epub::ContentBlock;
use epub::ListItem;
use epub::TableCell;
use epub::TextSpan;

use crate::measure::BlockMeasurer;
use crate::measure::block_height;
use crate::render::BlockHighlight;
use crate::render::RenderContext;
use crate::render::RenderMessage;
use crate::render::render_partial_list;
use crate::render::render_partial_paragraph;
use crate::render::render_partial_preformatted;
use crate::render::{
    self,
};

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
/// - table: row `child`, and `char_offset` 1 when the part starting there
///   repeats the table's header rows above it, 0 otherwise.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlockCut {
    pub child: usize,
    pub char_offset: usize,
}

/// Position within a chapter, independent of the current page dimensions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct PageAnchor {
    pub block: usize,
    pub cut: BlockCut,
}

/// A page is a contiguous range of blocks from the chapter's block list.
/// Stored as \[start .. end) (exclusive end).
#[derive(Clone, Debug)]
pub struct PageRange {
    pub start: usize,
    /// Where in `blocks[start]` the page starts (the default cut: at its
    /// beginning).
    pub start_cut: BlockCut,
    pub end: usize,
    /// Where in `blocks[end - 1]` the page ends (`None`: at its end).
    pub end_cut: Option<BlockCut>,
}

/// Cached pagination layout for a chapter at a particular viewport size.
#[derive(Clone, Debug)]
pub struct PaginationLayout {
    /// The available content height (in pixels) used to compute pages.
    pub page_height: f32,
    /// The available content width (in pixels) used for height estimation.
    pub page_width: f32,
    /// Computed page ranges.
    pub pages: Vec<PageRange>,
}

impl PageRange {
    pub fn anchor(&self) -> PageAnchor {
        PageAnchor {
            block: self.start,
            cut: self.start_cut,
        }
    }
}

impl PaginationLayout {
    /// Find the page beginning at or immediately before the reading position.
    pub fn page_at(&self, anchor: PageAnchor) -> usize {
        self.pages
            .partition_point(|page| page.anchor() <= anchor)
            .saturating_sub(1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageTurn {
    Stay,
    Page(usize),
    NextChapter,
    PreviousChapter,
}

/// Find the destination of a page turn. The preceding chapter's layout
/// determines where its final spread begins.
pub fn page_turn(
    current: usize,
    page_count: usize,
    step: usize,
    chapter: usize,
    chapter_count: usize,
    forward: bool,
) -> PageTurn {
    if forward {
        let next = current.saturating_add(step);
        if next < page_count {
            PageTurn::Page(next)
        } else if chapter + 1 < chapter_count {
            PageTurn::NextChapter
        } else {
            PageTurn::Stay
        }
    } else if current > 0 {
        PageTurn::Page(current.saturating_sub(step))
    } else if chapter > 0 {
        PageTurn::PreviousChapter
    } else {
        PageTurn::Stay
    }
}

pub fn last_spread(page_count: usize, step: usize) -> usize {
    (page_count.saturating_sub(1) / step) * step
}

/// Return the byte offset in `text` corresponding to `char_offset` Unicode code points.
/// Returns `text.len()` when `char_offset >= text.chars().count()`.
pub fn char_offset_to_byte(text: &str, char_offset: usize) -> usize {
    text.char_indices()
        .nth(char_offset)
        .map(|(i, _)| i)
        .unwrap_or(text.len())
}

/// Extract the character range `[start_char .. end_char)` from a `TextSpan` slice,
/// preserving all span metadata (style, link, color, font_size_em).
pub(crate) fn slice_spans(spans: &[TextSpan], start_char: usize, end_char: usize) -> Vec<TextSpan> {
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
        ContentBlock::Paragraph { text, spans, style } => ParagraphContent {
            text,
            spans,
            style: &ctx.paragraph_style(style),
        }
        .height_from(from.char_offset, content_width, ctx.font_size, ctx.family),
        _ => measurer.height(
            render_block_part(ctx, block, from, None, BlockHighlight::None),
            content_width,
        ),
    }
}

/// Height of `block` drawn by `ctx` `content_width` pixels wide, shown from
/// `from` on, as [`effective_block_height`] measures it when that is at most
/// `limit` pixels; otherwise a height above `limit`.
///
/// A table, list, code block or container continued from the previous page
/// is measured only up to where it is cut to fit twice `limit`, when that
/// part is already taller than `limit`, so a block spanning many pages is
/// not measured to its end on each of them.
fn block_height_up_to(
    measurer: &mut BlockMeasurer,
    ctx: &RenderContext<'_>,
    block: &ContentBlock,
    from: BlockCut,
    content_width: f32,
    limit: f32,
) -> f32 {
    let bounded = from != BlockCut::default() && !matches!(block, ContentBlock::Paragraph { .. });
    if bounded {
        let twice = 2.0 * limit;
        let end = match block {
            ContentBlock::Table { rows } => split_table(
                ctx,
                measurer,
                rows,
                from,
                from.char_offset > 0,
                content_width,
                twice,
            ),
            _ => try_split_block(ctx, measurer, block, content_width, twice, from)
                .filter(|cut| *cut > from),
        };
        if let Some(end) = end {
            // The part before `end` is drawn as the start of the rest of the
            // block, so the rest is at least as tall.
            let height = measurer.height(
                render_block_part(ctx, block, from, Some(end), BlockHighlight::None),
                content_width,
            );
            if height > limit {
                #[cfg(test)]
                debug_assert!(
                    effective_block_height(measurer, ctx, block, from, content_width) >= height,
                    "the start of a block is taller than the block"
                );
                return height;
            }
        }
    }
    effective_block_height(measurer, ctx, block, from, content_width)
}

/// The part of `block` from `from` up to `to` (`None`: to its end), as `ctx`
/// draws it on a page.
pub fn render_block_part<'a>(
    ctx: &RenderContext<'a>,
    block: &'a ContentBlock,
    from: BlockCut,
    to: Option<BlockCut>,
    highlight: BlockHighlight,
) -> Element<'a, RenderMessage> {
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
    if let ContentBlock::Table { rows } = block {
        return render::apply_highlight(
            render::render_table_rows(rows, from, to, ctx.font_size, ctx.family),
            highlight,
        );
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
                &ctx.paragraph_style(style),
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
                (ctx.copy_code_label)(),
            )
        }
        _ => ctx.render_block(block, highlight),
    }
}

/// The items of list `block`, and the number of its first item when it is
/// ordered; `None` when `block` is not a list.
pub fn list_parts(block: &ContentBlock) -> Option<(&[ListItem], Option<u32>)> {
    match block {
        ContentBlock::UnorderedList { items } => Some((items, None)),
        ContentBlock::OrderedList { start, items } => Some((items, Some(*start))),
        _ => None,
    }
}

/// The cut in container child `child` that a container's cut at
/// `char_offset` in that child stands for (see [`BlockCut`]).
pub fn container_child_cut(child: &ContentBlock, char_offset: usize) -> BlockCut {
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

/// Character offset at which code block `text` (styled by `spans`), shown
/// from character `start` on `content_width` pixels wide, must continue on
/// the next page, so that the lines before it fit in `available_height`
/// pixels: the start of the first line that does not fit. Returns `None`
/// when not even one line fits, or when all lines fit.
///
/// Only the first lines of the rest are laid out, more of them until one
/// does not fit: each line is laid out on its own, so they are laid out as
/// in the whole rest.
fn split_preformatted(
    text: &str,
    spans: &[TextSpan],
    start: usize,
    content_width: f32,
    font_size: f32,
    available_height: f32,
) -> Option<usize> {
    let start = start.min(text.chars().count());
    let padding = render::preformatted_padding();
    let available = available_height - padding.y();
    let rest = &text[char_offset_to_byte(text, start)..];
    let split = |shown: &str| {
        let spans = render::owned_paragraph_spans(
            shown.to_string(),
            slice_spans(spans, start, start + shown.chars().count()),
            cosmic::font::mono().family,
            font_size,
        );
        let layout = render::layout_preformatted(&spans, font_size, content_width - padding.x());
        let overflows = layout
            .buffer()
            .layout_runs()
            .any(|run| run.line_top + run.line_height > available);
        (
            overflows,
            first_overflowing_char(&layout, available).map(|offset| start + offset),
        )
    };
    let mut lines = PREFORMATTED_SPLIT_LINES;
    let offset = loop {
        let shown = preformatted_first_lines(rest, lines);
        let (overflows, offset) = split(shown);
        if overflows || shown.len() == rest.len() {
            break offset;
        }
        lines *= 2;
    };
    #[cfg(test)]
    debug_assert_eq!(
        offset,
        split(rest).1,
        "the first lines of a code block split as the whole block"
    );
    offset
}

/// Number of lines of a code block [`split_preformatted`] lays out first.
const PREFORMATTED_SPLIT_LINES: usize = 64;

/// The first `lines` lines of code block text `text`, without the line
/// ending after the last of them; all of `text` when it has no more lines.
fn preformatted_first_lines(text: &str, lines: usize) -> &str {
    let Some((end, _)) = text.match_indices('\n').nth(lines.saturating_sub(1)) else {
        return text;
    };
    let shown = &text[..end];
    shown.strip_suffix('\r').unwrap_or(shown)
}

/// End of the part of code block `text` shown from character `start` up to
/// character `end`. A part ending at a line boundary leaves out the line
/// ending (`\r\n`, `\n` or `\r`), so it draws exactly its lines, without an
/// empty last one.
pub fn preformatted_part_end(text: &str, start: usize, end: usize) -> usize {
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
        let child_ctx = ctx.child_context(block);
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
                let layout = render::layout_paragraph(
                    &spans,
                    &ctx.paragraph_style(&BlockStyle::default()),
                    ctx.family,
                    child_ctx.font_size,
                    width,
                );
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

/// Whether table `rows`, drawn by `ctx` `content_width` pixels wide, repeats
/// its header rows above the rows it continues with on later pages: when it
/// has header rows and body rows, and its header rows with its first body
/// row fit on an empty page `page_height` pixels high.
fn table_repeats_header(
    ctx: &RenderContext<'_>,
    measurer: &mut BlockMeasurer,
    rows: &[Vec<TableCell>],
    content_width: f32,
    page_height: f32,
) -> bool {
    let header = render::table_header_rows(rows);
    if header == 0 || header == rows.len() {
        return false;
    }
    let first_body_row = BlockCut {
        child: header + 1,
        char_offset: 0,
    };
    let height = measurer.height(
        render::render_table_rows(
            rows,
            BlockCut::default(),
            Some(first_body_row),
            ctx.font_size,
            ctx.family,
        ),
        content_width,
    );
    height <= page_height
}

/// The first row a page continuing table `rows`, shown from `from` on, may
/// start at: the row after the first shown row, or, when the table repeats
/// its header rows (`repeat_header`) and the part starts among them, the row
/// after its first body row, so the header rows are never left alone.
fn first_table_cut_row(rows: &[Vec<TableCell>], from: BlockCut, repeat_header: bool) -> usize {
    let header = render::table_header_rows(rows);
    if repeat_header && from.child < header {
        header + 1
    } else {
        from.child + 1
    }
}

/// Where to cut table `rows`, drawn by `ctx` and shown from `from` on, so
/// that the part before the cut fits in `available_height` pixels: before
/// the first row that does not fit, counting the header rows the part
/// repeats. `repeat_header`: whether the parts after the cut repeat the
/// header rows (see [`BlockCut`]). Returns `None` when all of it fits, or
/// when not even the rows before [`first_table_cut_row`] fit.
fn split_table(
    ctx: &RenderContext<'_>,
    measurer: &mut BlockMeasurer,
    rows: &[Vec<TableCell>],
    from: BlockCut,
    repeat_header: bool,
    content_width: f32,
    available_height: f32,
) -> Option<BlockCut> {
    let mut used = 0.0;
    for part in render::table_part_rows(rows, from, None) {
        used += measurer.height(
            render::render_table_row(&rows[part.index], part.divider, ctx.font_size, ctx.family),
            content_width,
        );
        if used > available_height {
            return (part.index >= first_table_cut_row(rows, from, repeat_header)).then_some(
                BlockCut {
                    child: part.index,
                    char_offset: usize::from(repeat_header),
                },
            );
        }
    }
    None
}

/// Where a page holding only the first shown row of table `rows`, shown from
/// `from` on, ends when more of the table follows (see
/// [`first_table_cut_row`]); `None` otherwise.
fn after_first_table_row(
    rows: &[Vec<TableCell>],
    from: BlockCut,
    repeat_header: bool,
) -> Option<BlockCut> {
    let child = first_table_cut_row(rows, from, repeat_header);
    (child < rows.len()).then_some(BlockCut {
        child,
        char_offset: usize::from(repeat_header),
    })
}

/// Where to cut `block`, drawn by `ctx` and shown from `from` on, so that the
/// part before the cut fits in `available_height` pixels: at the start of the
/// first line that does not fit, or between the block's children (see
/// [`BlockCut`]). Returns `None` when the block cannot be cut there: it is
/// not splittable, not even one line fits, or all of it fits.
pub fn try_split_block(
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
        ContentBlock::Paragraph { text, spans, style } => ParagraphContent {
            text,
            spans,
            style: &ctx.paragraph_style(style),
        }
        .split_from(
            from.char_offset,
            content_width,
            font_size,
            family,
            available_height,
        ),
        ContentBlock::Preformatted { text, spans, .. } => split_preformatted(
            text,
            spans,
            from.char_offset,
            content_width,
            font_size,
            available_height,
        ),
        _ => None,
    };
    char_offset.map(|char_offset| BlockCut {
        child: 0,
        char_offset,
    })
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
/// shown child. Tables are split between their rows; each page continuing a
/// table repeats its header rows above its rows, unless the header rows with
/// the first body row do not fit on a page, and a row taller than a page
/// gets a page of its own. A table inside a container is not split. Other
/// block types that exceed `page_height` get their own page unchanged.
///
/// A heading is not left as the last block of a page: when not even the
/// first part of the block after it (a line, list item, table row or other
/// smallest part it is cut into) fits below it, the heading starts the next
/// page with that block, together with the headings right before it (a part
/// heading above a section heading). The anchors among and right before
/// them (link targets, which take no room) move along, as do the anchors
/// right before any block starting the next page, so a link to them opens
/// the page showing their block. They stay where they are when they do not
/// fit on a page with that part, and when nothing else is on the page. This
/// applies to the chapter's own headings, not to headings inside block
/// quotes, footnotes or figures.
///
/// Blocks are measured as `ctx` draws them `page_width` wide, `block_spacing`
/// apart; blocks measuring 0 take no spacing, as the page column leaves them
/// out.
pub fn paginate_blocks(
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
        let block_h = block_height_up_to(measurer, ctx, block, from, page_width, page_height);
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
        let cut = if let ContentBlock::Table { rows } = block {
            let repeat_header = if from == BlockCut::default() {
                table_repeats_header(ctx, measurer, rows, page_width, page_height)
            } else {
                from.char_offset > 0
            };
            split_table(
                ctx,
                measurer,
                rows,
                from,
                repeat_header,
                page_width,
                available,
            )
            .or_else(|| {
                // A first shown row too tall for a page is placed alone.
                is_first_on_page
                    .then(|| after_first_table_row(rows, from, repeat_header))
                    .flatten()
            })
        } else {
            try_split_block(ctx, measurer, block, page_width, available, from)
                .filter(|cut| *cut > from)
                .or_else(|| {
                    // A container's first shown child (or a nested
                    // container's first shown child) that cannot be split is
                    // placed alone, to overflow the page on its own.
                    is_first_on_page
                        .then(|| after_first_container_child(ctx, block, from))
                        .flatten()
                })
        };
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
                // Nothing fits on this page — close it and start block on
                // next page, with the headings right before it.
                let end = headings_before(blocks, page_start, i)
                    .filter(|&headings| {
                        fits_with_headings(
                            blocks,
                            headings,
                            page_height,
                            page_width,
                            block_spacing,
                            ctx,
                            measurer,
                        )
                    })
                    .unwrap_or(i);
                pages.push(PageRange {
                    start: page_start,
                    start_cut: page_start_cut,
                    end,
                    end_cut: None,
                });
                page_start = end;
                page_start_cut = BlockCut::default();
                i = end;
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

/// Whether `block` is a heading.
pub fn is_heading(block: &ContentBlock) -> bool {
    matches!(block, ContentBlock::Heading { .. })
}

/// Whether `block` is a link target, which takes no room on the page.
pub fn is_anchor(block: &ContentBlock) -> bool {
    matches!(block, ContentBlock::Anchor { .. })
}

/// Index of the first of the headings and anchors right before block `end`
/// on the page starting at block `page_start`; `None` when block `end - 1`
/// is neither, or when the page holds nothing but these blocks.
fn headings_before(blocks: &[ContentBlock], page_start: usize, end: usize) -> Option<usize> {
    let headings = blocks[page_start..end]
        .iter()
        .rev()
        .take_while(|block| is_heading(block) || is_anchor(block))
        .count();
    (headings > 0 && headings < end - page_start).then_some(end - headings)
}

/// Whether the headings and anchors from block `start` on, with the first
/// part of the block after them (a line, list item, table row or other
/// smallest part it is cut into), fit on an empty page `page_height` pixels
/// high. Headings ending the chapter fit when they fit on their own.
fn fits_with_headings(
    blocks: &[ContentBlock],
    start: usize,
    page_height: f32,
    page_width: f32,
    block_spacing: f32,
    ctx: &RenderContext<'_>,
    measurer: &mut BlockMeasurer,
) -> bool {
    let from = BlockCut::default();
    let mut used = 0.0f32;
    for block in &blocks[start..] {
        let block_h = effective_block_height(measurer, ctx, block, from, page_width);
        let spacing = if used > 0.0 && block_h > 0.0 {
            block_spacing
        } else {
            0.0
        };
        let available = page_height - used - spacing;
        if !is_heading(block) && !is_anchor(block) {
            return block_h <= available
                || first_part_fits(ctx, measurer, block, page_width, page_height, available);
        }
        if block_h > available {
            return false;
        }
        used += spacing + block_h;
    }
    true
}

/// Whether the first part of `block` (a line, list item, table row or other
/// smallest part it is cut into) fits in `available` pixels at the bottom of
/// a page `page_height` pixels high.
fn first_part_fits(
    ctx: &RenderContext<'_>,
    measurer: &mut BlockMeasurer,
    block: &ContentBlock,
    page_width: f32,
    page_height: f32,
    available: f32,
) -> bool {
    let from = BlockCut::default();
    if available <= 0.0 {
        return false;
    }
    match block {
        ContentBlock::Table { rows } => {
            let repeat_header = table_repeats_header(ctx, measurer, rows, page_width, page_height);
            split_table(
                ctx,
                measurer,
                rows,
                from,
                repeat_header,
                page_width,
                available,
            )
            .is_some()
        }
        _ => try_split_block(ctx, measurer, block, page_width, available, from)
            .is_some_and(|cut| cut > from),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn justified_paragraph_keeps_page_cuts_and_fits_each_fragment() {
        let width = 240.0;
        let height = 75.0;
        let ctx = RenderContext {
            justify_paragraphs: false,
            font_size: 16.0,
            family: font::Family::SansSerif,
            max_image_height: height,
            copy_code_label: String::new,
            image_handles: &Default::default(),
        };
        let text =
            "The reader can justify prose without changing where the lines wrap. ".repeat(12);
        let split = 40;
        let spans = [
            TextSpan {
                text: text[..split].to_owned(),
                style: epub::InlineStyle {
                    bold: true,
                    ..Default::default()
                },
                link: None,
                color: None,
                font_size_em: None,
            },
            TextSpan {
                text: text[split..].to_owned(),
                style: epub::InlineStyle {
                    italic: true,
                    ..Default::default()
                },
                link: Some("#note".into()),
                color: None,
                font_size_em: None,
            },
        ];
        let paragraph = |text_align| ContentBlock::Paragraph {
            text: text.clone(),
            spans: spans.to_vec(),
            style: BlockStyle {
                text_align,
                ..BlockStyle::default()
            },
        };
        let left = [paragraph(None)];
        let mut bold_text = String::new();
        let mut linked_text = String::new();
        let justified = [paragraph(Some(epub::TextAlign::Justify))];
        let mut measurer = BlockMeasurer::default();
        let left_layout = paginate_blocks(&left, height, width, 0.0, &ctx, &mut measurer);
        let justified_layout = paginate_blocks(&justified, height, width, 0.0, &ctx, &mut measurer);

        assert!(justified_layout.pages.len() > 1);
        assert_eq!(justified_layout.pages.len(), left_layout.pages.len());
        for (left_page, page) in left_layout.pages.iter().zip(&justified_layout.pages) {
            assert_eq!(page.start_cut, left_page.start_cut);
            assert_eq!(page.end_cut, left_page.end_cut);
            let start = page.start_cut.char_offset;
            let end = page
                .end_cut
                .map_or(text.chars().count(), |cut| cut.char_offset);
            let fragments = slice_spans(&spans, start, end);
            for span in fragments {
                if span.style.bold {
                    assert!(!span.style.italic);
                    assert_eq!(span.link, None);
                    bold_text.push_str(&span.text);
                } else {
                    assert!(span.style.italic);
                    assert_eq!(span.link.as_deref(), Some("#note"));
                    linked_text.push_str(&span.text);
                }
            }
            let element = render_block_part(
                &ctx,
                &justified[0],
                page.start_cut,
                page.end_cut,
                BlockHighlight::None,
            );
            assert!(measurer.height(element, width) <= height);
        }
        assert_eq!(bold_text, spans[0].text);
        assert_eq!(linked_text, spans[1].text);
    }

    #[test]
    fn a_position_inside_a_split_block_resolves_to_its_page() {
        let first = BlockCut {
            child: 0,
            char_offset: 80,
        };
        let second = BlockCut {
            child: 0,
            char_offset: 130,
        };
        let layout = PaginationLayout {
            page_width: 360.0,
            page_height: 480.0,
            pages: vec![
                PageRange {
                    start: 2,
                    start_cut: BlockCut::default(),
                    end: 3,
                    end_cut: Some(first),
                },
                PageRange {
                    start: 2,
                    start_cut: first,
                    end: 3,
                    end_cut: Some(second),
                },
                PageRange {
                    start: 2,
                    start_cut: second,
                    end: 3,
                    end_cut: None,
                },
            ],
        };
        assert_eq!(
            layout.page_at(PageAnchor {
                block: 2,
                cut: BlockCut {
                    child: 0,
                    char_offset: 125
                },
            }),
            1
        );
        assert_eq!(
            layout.page_at(PageAnchor {
                block: 2,
                cut: second
            }),
            2
        );
    }

    #[test]
    fn turning_a_spread_crosses_chapters_at_the_edges() {
        assert_eq!(page_turn(2, 5, 2, 1, 3, true), PageTurn::Page(4));
        assert_eq!(page_turn(4, 5, 2, 1, 3, true), PageTurn::NextChapter);
        assert_eq!(page_turn(1, 5, 2, 1, 3, false), PageTurn::Page(0));
        assert_eq!(page_turn(0, 5, 2, 1, 3, false), PageTurn::PreviousChapter);
        assert_eq!(last_spread(6, 2), 4);
    }
}
