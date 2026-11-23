use cosmic::Apply;
use cosmic::Element;
use cosmic::cosmic_theme;
use cosmic::iced::Background;
use cosmic::iced::Border;
use cosmic::iced::Color;
use cosmic::iced::Font;
use cosmic::iced::Length;
use cosmic::iced::Padding;
use cosmic::iced::Pixels;
use cosmic::iced::Size;
use cosmic::iced::advanced::graphics::text::Paragraph;
use cosmic::iced::advanced::text::Paragraph as _;
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::alignment::Vertical;
use cosmic::iced::font;
use cosmic::iced::widget::rich_text;
use cosmic::iced::widget::span;
use cosmic::iced::widget::text::LineHeight;
use cosmic::iced::widget::text::Shaping;
use cosmic::iced::widget::text::Span;
use cosmic::iced::widget::text::Wrapping;
use cosmic::theme;
use cosmic::theme::Container;
use cosmic::widget;
use epub::BlockStyle;
use epub::ContentBlock;
use epub::InlineStyle;
use epub::ListItem;
use epub::TableCell;
use epub::TextAlign;
use epub::TextSpan;

use crate::ICON_SIZE;
use crate::fl;
use crate::page::epub_viewer::BlockCut;
use crate::page::epub_viewer::BlockHighlight;
use crate::page::epub_viewer::EpubViewerMessage;
use crate::page::epub_viewer::char_offset_to_byte;
use crate::page::epub_viewer::slice_spans;
use crate::page::image_viewer::ViewerImage;

#[derive(Clone, Copy)]
pub(super) struct RenderContext<'a> {
    pub font_size: f32,
    pub family: font::Family,
    pub max_image_height: f32,
    /// Stable image handles keyed by data pointer, to avoid re-creating handles
    /// each frame (which would produce a new ID and prevent async texture reuse).
    pub image_handles: &'a std::collections::HashMap<usize, widget::image::Handle>,
}

/// Line height of paragraph text. Rendering and pagination both use it, so
/// pagination measures paragraphs exactly as they are drawn.
const PARAGRAPH_LINE_HEIGHT: LineHeight = LineHeight::Relative(1.4);

/// Font size of a paragraph with `style` at the reader's `font_size`.
pub(super) fn paragraph_font_size(style: &BlockStyle, font_size: f32) -> f32 {
    style
        .font_size_em
        .map(|em| em * font_size)
        .unwrap_or(font_size)
}

/// Font size of a styled heading of `level` with `style` at the reader's
/// `font_size`.
pub(super) fn heading_font_size(level: u8, style: &BlockStyle, font_size: f32) -> f32 {
    let base_size = match level {
        1 => font_size * 2.0,
        2 => font_size * 1.75,
        3 => font_size * 1.5,
        4 => font_size * 1.25,
        _ => font_size * 1.125,
    };
    style
        .font_size_em
        .map(|em| em * base_size)
        .unwrap_or(base_size)
}

/// The spans a paragraph is drawn with: its styled spans, or its plain `text`
/// as a single span in the reader's font.
pub(super) fn paragraph_spans<'a>(
    text: &'a str,
    spans: &'a [TextSpan],
    family: font::Family,
    size: f32,
) -> Vec<Span<'a, EpubViewerMessage>> {
    if spans.is_empty() {
        vec![span(text).font(Font {
            family,
            ..Font::default()
        })]
    } else {
        spans.iter().map(|s| styled_span(s, family, size)).collect()
    }
}

/// Owned variant of [`paragraph_spans`], for paragraphs split across pages.
pub(super) fn owned_paragraph_spans(
    text: String,
    spans: Vec<TextSpan>,
    family: font::Family,
    size: f32,
) -> Vec<Span<'static, EpubViewerMessage>> {
    if spans.is_empty() {
        vec![span(text).font(Font {
            family,
            ..Font::default()
        })]
    } else {
        spans
            .into_iter()
            .map(|s| owned_styled_span(s, family, size))
            .collect()
    }
}

/// Draw paragraph `spans`.
fn paragraph_text<'a>(
    spans: Vec<Span<'a, EpubViewerMessage>>,
    style: &BlockStyle,
    family: font::Family,
    size: f32,
) -> Element<'a, EpubViewerMessage> {
    apply_text_align(
        rich_text(spans)
            .on_link_click(|m| m)
            .size(size)
            .line_height(PARAGRAPH_LINE_HEIGHT)
            .font(Font {
                family,
                ..Font::default()
            })
            .width(Length::Fill)
            .align_x(text_align_horizontal(style))
            .into(),
        style,
    )
}

/// Lay out paragraph `spans` `width` pixels wide exactly as [`paragraph_text`]
/// draws them, excluding the block margins (see [`block_margins`]).
pub(super) fn layout_paragraph(
    spans: &[Span<'_, EpubViewerMessage>],
    style: &BlockStyle,
    family: font::Family,
    size: f32,
    width: f32,
) -> Paragraph {
    Paragraph::with_spans(cosmic::iced::advanced::Text {
        content: spans,
        bounds: Size::new(width, f32::INFINITY),
        size: Pixels(size),
        line_height: PARAGRAPH_LINE_HEIGHT,
        font: Font {
            family,
            ..Font::default()
        },
        align_x: text_align_horizontal(style).into(),
        align_y: Vertical::Top,
        shaping: Shaping::Advanced,
        wrapping: Wrapping::default(),
        ellipsize: Default::default(),
    })
}

/// Render a partial paragraph (split at page boundary) using owned text and span data.
/// The returned element is self-contained — it does not borrow from any local variables.
pub(super) fn render_partial_paragraph<'a>(
    text: String,
    spans: Vec<TextSpan>,
    style: &'a BlockStyle,
    highlight: BlockHighlight,
    font_size: f32,
    family: font::Family,
) -> Element<'a, EpubViewerMessage> {
    let size = paragraph_font_size(style, font_size);
    apply_highlight(
        paragraph_text(
            owned_paragraph_spans(text, spans, family, size),
            style,
            family,
            size,
        ),
        highlight,
    )
}

/// Line height of code block text, relative to the reader's font size like
/// [`PARAGRAPH_LINE_HEIGHT`], so code lines keep the rhythm of the prose
/// around them. Rendering and pagination both use it.
const PREFORMATTED_LINE_HEIGHT: LineHeight = LineHeight::Relative(1.4);

/// Padding between a code block's background and its text.
pub(super) fn preformatted_padding() -> Padding {
    let cosmic_theme::Spacing {
        space_xxs, space_s, ..
    } = theme::active().cosmic().spacing;
    Padding::from([space_xxs, space_s])
}

/// Draw code block `spans` in the monospace font at the reader's font `size`,
/// wrapping lines too long for the page.
fn preformatted_text<'a>(
    spans: Vec<Span<'a, EpubViewerMessage>>,
    size: f32,
) -> Element<'a, EpubViewerMessage> {
    rich_text(spans)
        .on_link_click(|m| m)
        .size(size)
        .line_height(PREFORMATTED_LINE_HEIGHT)
        .font(cosmic::font::mono())
        .wrapping(Wrapping::WordOrGlyph)
        .width(Length::Fill)
        .into()
}

/// Lay out code block `spans` `width` pixels wide exactly as
/// [`preformatted_text`] draws them, excluding the padding (see
/// [`preformatted_padding`]).
pub(super) fn layout_preformatted(
    spans: &[Span<'_, EpubViewerMessage>],
    size: f32,
    width: f32,
) -> Paragraph {
    Paragraph::with_spans(cosmic::iced::advanced::Text {
        content: spans,
        bounds: Size::new(width, f32::INFINITY),
        size: Pixels(size),
        line_height: PREFORMATTED_LINE_HEIGHT,
        font: cosmic::font::mono(),
        align_x: Horizontal::Left.into(),
        align_y: Vertical::Top,
        shaping: Shaping::Advanced,
        wrapping: Wrapping::WordOrGlyph,
        ellipsize: Default::default(),
    })
}

/// A code block showing `spans`; `text_to_copy`, when `Some`, adds a copy
/// button that copies it.
fn code_block<'a>(
    spans: Vec<Span<'a, EpubViewerMessage>>,
    size: f32,
    text_to_copy: Option<String>,
) -> Element<'a, EpubViewerMessage> {
    let code_block = widget::container(preformatted_text(spans, size))
        .padding(preformatted_padding())
        .class(Container::Secondary)
        .width(Length::Fill);
    if let Some(text) = text_to_copy {
        let copy_button = widget::row::with_children(vec![
            widget::space::horizontal().width(Length::Fill).into(),
            widget::button::icon(widget::icon::from_name("edit-copy-symbolic").size(ICON_SIZE))
                .on_press(EpubViewerMessage::CopyCodeBlock(text))
                .tooltip(fl!("epub-viewer-copy-code"))
                .into(),
        ]);
        cosmic::iced::widget::stack(vec![code_block.into(), copy_button.into()]).into()
    } else {
        code_block.into()
    }
}

/// Render a partial preformatted block (split at a page boundary) using owned data.
/// `text` and `spans` must already be sliced to the visible char range by the caller.
/// `full_text_to_copy`, when `Some`, adds a copy button that copies the entire block content.
pub(super) fn render_partial_preformatted(
    text: String,
    spans: Vec<TextSpan>,
    font_size: f32,
    full_text_to_copy: Option<String>,
) -> Element<'static, EpubViewerMessage> {
    code_block(
        owned_paragraph_spans(text, spans, cosmic::font::mono().family, font_size),
        font_size,
        full_text_to_copy,
    )
}

pub(super) fn apply_highlight<'a>(
    inner: Element<'a, EpubViewerMessage>,
    highlight: BlockHighlight,
) -> Element<'a, EpubViewerMessage> {
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

impl<'a> RenderContext<'a> {
    pub(super) fn render_block(
        &self,
        block: &'a ContentBlock,
        highlight: BlockHighlight,
    ) -> Element<'a, EpubViewerMessage> {
        apply_highlight(self.render_block_inner(block), highlight)
    }

    fn render_block_inner(&self, block: &'a ContentBlock) -> Element<'a, EpubViewerMessage> {
        let font_size = self.font_size;
        let family = self.family;
        let max_image_height = self.max_image_height;

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
                let size = heading_font_size(*level, style, font_size);
                paragraph_text(
                    paragraph_spans(text, spans, family, size),
                    style,
                    family,
                    size,
                )
            }
            ContentBlock::Paragraph { spans, text, style } => {
                let size = paragraph_font_size(style, font_size);
                paragraph_text(
                    paragraph_spans(text, spans, family, size),
                    style,
                    family,
                    size,
                )
            }
            ContentBlock::Preformatted { text, spans, .. } => code_block(
                paragraph_spans(text, spans, cosmic::font::mono().family, font_size),
                font_size,
                Some(text.clone()),
            ),
            ContentBlock::BlockQuote { .. }
            | ContentBlock::Footnote { .. }
            | ContentBlock::Figure { .. } => {
                self.render_partial_container(block, BlockCut::default(), None)
            }
            ContentBlock::UnorderedList { .. } | ContentBlock::OrderedList { .. } => {
                let (items, ordered_start) = super::list_parts(block).unwrap_or_default();
                render_partial_list(
                    items,
                    ordered_start,
                    BlockCut::default(),
                    None,
                    font_size,
                    family,
                )
            }
            ContentBlock::Image { data, .. } if !data.is_empty() => {
                let key = data.as_ptr() as usize;
                let handle = self
                    .image_handles
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| widget::image::Handle::from_bytes(data.clone()));
                let container = widget::image(handle.clone())
                    .width(Length::Shrink)
                    .content_fit(cosmic::iced::ContentFit::ScaleDown)
                    .apply(widget::container)
                    .width(Length::Fill)
                    .max_height(max_image_height)
                    .align_x(Horizontal::Center);
                let (natural_width, natural_height) =
                    image::ImageReader::new(std::io::Cursor::new(data.as_slice()))
                        .with_guessed_format()
                        .ok()
                        .and_then(|r| r.into_dimensions().ok())
                        .unwrap_or((0u32, 0u32));
                widget::mouse_area(container)
                    .on_press(EpubViewerMessage::OpenImageViewer(ViewerImage::Raster {
                        handle,
                        natural_width,
                        natural_height,
                    }))
                    .into()
            }
            ContentBlock::Image { alt, .. } => {
                if !alt.is_empty() {
                    widget::text::body(format!("[{alt}]"))
                        .font(font)
                        .width(Length::Fill)
                        .into()
                } else {
                    widget::Space::new().width(Length::Fill).height(0).into()
                }
            }
            ContentBlock::Svg { content, .. } => {
                // SVG image references are resolved at load time; render directly.
                let handle = widget::svg::Handle::from_memory(content.as_bytes().to_vec());
                let container = widget::svg(handle.clone())
                    .width(Length::Shrink)
                    .content_fit(cosmic::iced::ContentFit::ScaleDown)
                    .apply(widget::container)
                    .width(Length::Fill)
                    .max_height(max_image_height)
                    .align_x(Horizontal::Center);
                widget::mouse_area(container)
                    .on_press(EpubViewerMessage::OpenImageViewer(ViewerImage::Svg(handle)))
                    .into()
            }
            ContentBlock::Table { rows } => {
                render_table_rows(rows, BlockCut::default(), None, font_size, family)
            }
            ContentBlock::HorizontalRule => widget::divider::horizontal::default().into(),
            ContentBlock::Anchor { .. } => widget::Space::new()
                .width(Length::Fixed(0.0))
                .height(Length::Fixed(0.0))
                .into(),
        }
    }

    /// The context a footnote draws its child blocks with: footnote text
    /// size, images at their natural height.
    pub(super) fn footnote_context(&self) -> Self {
        RenderContext {
            font_size: footnote_font_size(self.font_size),
            max_image_height: f32::MAX,
            ..*self
        }
    }

    /// Draw container `block` (see [`container_children`]) from `from` up to
    /// `to` (`None`: to its end). Every part keeps the container's padding and
    /// background; a figure draws its caption on its last part only.
    pub(super) fn render_partial_container(
        &self,
        block: &'a ContentBlock,
        from: BlockCut,
        to: Option<BlockCut>,
    ) -> Element<'a, EpubViewerMessage> {
        let children = container_children(block).unwrap_or_default();
        let end = to
            .map_or(children.len(), |to| {
                to.child + usize::from(to.char_offset > 0)
            })
            .min(children.len());
        let mut col = widget::column::with_capacity(end.saturating_sub(from.child) + 1)
            .spacing(container_child_spacing())
            .width(Length::Fill);
        for (index, child) in children.iter().enumerate().take(end).skip(from.child) {
            let child_from = if index == from.child {
                super::container_child_cut(child, from.char_offset)
            } else {
                BlockCut::default()
            };
            let child_to = to
                .filter(|to| to.child == index)
                .map(|to| super::container_child_cut(child, to.char_offset));
            col = col.push(self.render_container_child(block, index, child_from, child_to));
        }
        if to.is_none()
            && let Some(caption) = self.figure_caption(block)
        {
            col = col.push(caption);
        }
        match block {
            ContentBlock::BlockQuote { .. } => widget::container(col)
                .padding(container_padding(block))
                .width(Length::Fill)
                .into(),
            ContentBlock::Footnote { .. } => widget::container(col)
                .padding(container_padding(block))
                .class(Container::Secondary)
                .width(Length::Fill)
                .into(),
            _ => col.into(),
        }
    }

    /// Draw child `index` of container `container` from `from` up to `to`
    /// (`None`: to its end). A footnote draws its paragraphs after their
    /// prefix (see [`footnote_paragraph_spans`]), and its other children in
    /// its [`footnote_context`](Self::footnote_context).
    pub(super) fn render_container_child(
        &self,
        container: &'a ContentBlock,
        index: usize,
        from: BlockCut,
        to: Option<BlockCut>,
    ) -> Element<'a, EpubViewerMessage> {
        let child = &container_children(container).unwrap_or_default()[index];
        match (container, child) {
            (ContentBlock::Footnote { id, .. }, ContentBlock::Paragraph { text, spans, .. }) => {
                paragraph_text(
                    footnote_paragraph_spans(
                        id,
                        index,
                        text,
                        spans,
                        from.char_offset,
                        to.map_or(usize::MAX, |to| to.char_offset),
                        self.family,
                        self.font_size,
                    ),
                    &BlockStyle::default(),
                    self.family,
                    footnote_font_size(self.font_size),
                )
            }
            (ContentBlock::Footnote { .. }, _) => super::render_block_part(
                &self.footnote_context(),
                child,
                from,
                to,
                BlockHighlight::None,
            ),
            _ => super::render_block_part(self, child, from, to, BlockHighlight::None),
        }
    }

    /// The caption figure `block` draws below its blocks; `None` when it has
    /// none or is not a figure.
    pub(super) fn figure_caption(
        &self,
        block: &'a ContentBlock,
    ) -> Option<Element<'a, EpubViewerMessage>> {
        let ContentBlock::Figure {
            caption,
            caption_text,
            ..
        } = block
        else {
            return None;
        };
        let family = self.family;
        let caption_size = (self.font_size * 0.85).max(10.0);
        if !caption.is_empty() {
            let iced_spans: Vec<_> = caption
                .iter()
                .map(|s| styled_span(s, family, caption_size))
                .collect();
            Some(
                widget::container(
                    rich_text(iced_spans)
                        .on_link_click(|m| m)
                        .size(caption_size),
                )
                .width(Length::Fill)
                .align_x(Horizontal::Center)
                .into(),
            )
        } else if !caption_text.is_empty() {
            Some(
                widget::text::caption(caption_text.as_str())
                    .size(caption_size)
                    .font(Font {
                        family,
                        style: font::Style::Italic,
                        ..Font::default()
                    })
                    .width(Length::Fill)
                    .align_x(Horizontal::Center)
                    .into(),
            )
        } else {
            None
        }
    }
}

/// Padding between the edge of container `block` and its children.
pub(super) fn container_padding(block: &ContentBlock) -> Padding {
    match block {
        ContentBlock::BlockQuote { .. } | ContentBlock::Footnote { .. } => {
            let cosmic_theme::Spacing {
                space_xxs, space_s, ..
            } = theme::active().cosmic().spacing;
            Padding::from([space_xxs, space_s])
        }
        _ => Padding::ZERO,
    }
}

/// Space between the children of a container.
pub(super) fn container_child_spacing() -> f32 {
    theme::active().cosmic().spacing.space_xxs as f32
}

/// The divider a table draws above a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TableDivider {
    Light,
    /// Above the first body row after a header row.
    Heavy,
}

/// A row a part of a table draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TableRowPart {
    /// Index of the row in the table.
    pub index: usize,
    /// The divider drawn above the row.
    pub divider: TableDivider,
}

/// Whether table row `row` is a header row: one with a header cell.
fn is_table_header_row(row: &[TableCell]) -> bool {
    row.iter().any(|cell| cell.is_header)
}

/// Number of header rows at the start of table `rows`.
pub(super) fn table_header_rows(rows: &[Vec<TableCell>]) -> usize {
    rows.iter()
        .take_while(|row| is_table_header_row(row))
        .count()
}

/// The rows the part of table `rows` from `from` up to `to` (`None`: to its
/// end) draws, in order: the table's header rows when the part repeats them
/// (see [`BlockCut`]), then rows `from.child` up to `to.child`. Each row has a
/// light divider above it, except the first body row after a header row,
/// which has a heavy one.
pub(super) fn table_part_rows(
    rows: &[Vec<TableCell>],
    from: BlockCut,
    to: Option<BlockCut>,
) -> Vec<TableRowPart> {
    let end = to.map_or(rows.len(), |to| to.child).min(rows.len());
    let start = from.child.min(end);
    let repeated = if start > 0 && from.char_offset > 0 {
        table_header_rows(rows).min(start)
    } else {
        0
    };
    let mut seen_header = false;
    let mut heavy_drawn = false;
    (0..repeated)
        .chain(start..end)
        .map(|index| {
            let is_header = is_table_header_row(&rows[index]);
            let divider = if seen_header && !is_header && !heavy_drawn {
                heavy_drawn = true;
                TableDivider::Heavy
            } else {
                TableDivider::Light
            };
            seen_header |= is_header;
            TableRowPart { index, divider }
        })
        .collect()
}

/// Draw the part of table `rows` from `from` up to `to` (`None`: to its end):
/// the rows [`table_part_rows`] lists, one below the other.
pub(super) fn render_table_rows(
    rows: &[Vec<TableCell>],
    from: BlockCut,
    to: Option<BlockCut>,
    font_size: f32,
    family: font::Family,
) -> Element<'_, EpubViewerMessage> {
    let parts = table_part_rows(rows, from, to);
    let mut col = widget::column::with_capacity(parts.len())
        .spacing(0)
        .width(Length::Fill);
    for part in parts {
        col = col.push(render_table_row(
            &rows[part.index],
            part.divider,
            font_size,
            family,
        ));
    }
    widget::container(col).width(Length::Fill).into()
}

/// Draw table row `row` below `divider`, its header cells in bold.
pub(super) fn render_table_row(
    row: &[TableCell],
    divider: TableDivider,
    font_size: f32,
    family: font::Family,
) -> Element<'_, EpubViewerMessage> {
    let cosmic_theme::Spacing {
        space_xxs, space_s, ..
    } = theme::active().cosmic().spacing;

    let mut row_widget = widget::Row::new().width(Length::Fill);

    for cell in row {
        let cell_spans: Vec<cosmic::iced::widget::text::Span<'_, EpubViewerMessage>> =
            if !cell.spans.is_empty() {
                cell.spans
                    .iter()
                    .map(|s| {
                        let mut sp = styled_span(s, family, font_size);
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
                .on_link_click(|m| m)
                .size(font_size)
                .width(Length::Fill)
                .into()
        } else {
            widget::Space::new().width(Length::Fill).height(0).into()
        };

        row_widget = row_widget.push(
            widget::container(cell_content)
                .padding([space_xxs, space_s])
                .width(Length::FillPortion(1)),
        );
    }

    let divider: Element<'_, EpubViewerMessage> = match divider {
        TableDivider::Light => widget::divider::horizontal::light().into(),
        TableDivider::Heavy => widget::divider::horizontal::heavy().into(),
    };
    widget::column::with_capacity(2)
        .push(divider)
        .push(row_widget)
        .width(Length::Fill)
        .into()
}

/// Map `BlockStyle.text_align` to an iced `Horizontal` alignment.
fn text_align_horizontal(style: &BlockStyle) -> cosmic::iced::alignment::Horizontal {
    match style.text_align {
        Some(TextAlign::Center) => cosmic::iced::alignment::Horizontal::Center,
        Some(TextAlign::Right) => cosmic::iced::alignment::Horizontal::Right,
        _ => cosmic::iced::alignment::Horizontal::Left,
    }
}

/// Top and bottom margin in pixels from `style` (em converted at a 16px base).
pub(super) fn block_margins(style: &BlockStyle) -> (f32, f32) {
    (
        style.margin_top_em.unwrap_or(0.0) * 16.0,
        style.margin_bottom_em.unwrap_or(0.0) * 16.0,
    )
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
    let (top, bottom) = block_margins(style);
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

fn style_span<'a>(
    mut s: cosmic::iced::widget::text::Span<'a, EpubViewerMessage>,
    family: font::Family,
    font_size: f32,
    inline: &InlineStyle,
    color: Option<[u8; 3]>,
    font_size_em: Option<f32>,
    link: Option<String>,
) -> cosmic::iced::widget::text::Span<'a, EpubViewerMessage> {
    let weight = if inline.bold {
        font::Weight::Bold
    } else {
        font::Weight::Normal
    };
    let font_style = if inline.italic {
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
    if inline.underline {
        s = s.underline(true);
    }
    if inline.strikethrough {
        s = s.strikethrough(true);
    }
    if inline.monospaced {
        s = s.font(cosmic::font::mono());
        s = s.background(Background::Color(
            cosmic::theme::active()
                .cosmic()
                .secondary(false)
                .base
                .into(),
        ));
    }
    if let Some([r, g, b]) = color {
        s = s.color(cosmic::iced::Color::from_rgb8(r, g, b));
    }
    if let Some(em) = font_size_em {
        s = s.size(em * font_size);
    }
    if let Some(href) = link {
        s = s.link(EpubViewerMessage::FollowLink(href));
        if color.is_none() {
            s = s.color(theme::active().cosmic().accent_color());
        }
    }
    s
}

fn owned_styled_span(
    ts: TextSpan,
    family: font::Family,
    font_size: f32,
) -> cosmic::iced::widget::text::Span<'static, EpubViewerMessage> {
    style_span(
        span(ts.text),
        family,
        font_size,
        &ts.style,
        ts.color,
        ts.font_size_em,
        ts.link,
    )
}

/// What a page shows of one list item: characters `start..end` of item
/// `index`, after `prefix` when it shows the item from its beginning.
#[derive(Debug)]
pub(super) struct ListItemPart {
    pub index: usize,
    pub prefix: Option<String>,
    pub start: usize,
    pub end: usize,
}

/// The prefix drawn before list item `index`: a bullet, or, in a list
/// numbered from `ordered_start`, the item's number.
fn list_item_prefix(ordered_start: Option<u32>, index: usize) -> String {
    match ordered_start {
        Some(start) => format!("  {}. ", start as usize + index),
        None => "  \u{2022} ".to_string(),
    }
}

/// The parts of list `items` (numbered from `ordered_start` when ordered)
/// shown from `from` up to `to` (`None`: to the end of the list). A cut at
/// character 0 of an item falls between items.
pub(super) fn list_item_parts(
    items: &[ListItem],
    ordered_start: Option<u32>,
    from: BlockCut,
    to: Option<BlockCut>,
) -> Vec<ListItemPart> {
    (from.child..items.len())
        .take_while(|&index| {
            to.is_none_or(|to| index < to.child || (index == to.child && to.char_offset > 0))
        })
        .map(|index| {
            let start = if index == from.child {
                from.char_offset
            } else {
                0
            };
            let end = match to {
                Some(to) if index == to.child => to.char_offset,
                _ => usize::MAX,
            };
            ListItemPart {
                index,
                prefix: (start == 0).then(|| list_item_prefix(ordered_start, index)),
                start,
                end,
            }
        })
        .collect()
}

/// The spans list item `part` of `item` is drawn with: its prefix, then the
/// item's text as a paragraph's.
pub(super) fn list_item_part_spans<'a>(
    item: &'a ListItem,
    part: &ListItemPart,
    family: font::Family,
    size: f32,
) -> Vec<Span<'a, EpubViewerMessage>> {
    prefixed_spans(
        part.prefix.clone(),
        &item.text,
        &item.spans,
        part.start,
        part.end,
        family,
        size,
    )
}

/// The spans characters `start..end` of a paragraph's `text` (styled by
/// `spans`) are drawn with, after `prefix`.
fn prefixed_spans<'a>(
    prefix: Option<String>,
    text: &'a str,
    spans: &'a [TextSpan],
    start: usize,
    end: usize,
    family: font::Family,
    size: f32,
) -> Vec<Span<'a, EpubViewerMessage>> {
    let prefix = prefix.map(|prefix| {
        span(prefix).font(Font {
            family,
            ..Font::default()
        })
    });
    let text = if start == 0 && end == usize::MAX {
        paragraph_spans(text, spans, family, size)
    } else {
        let char_count = text.chars().count();
        let start = start.min(char_count);
        let end = end.min(char_count);
        owned_paragraph_spans(
            text[char_offset_to_byte(text, start)..char_offset_to_byte(text, end)].to_string(),
            slice_spans(spans, start, end),
            family,
            size,
        )
    };
    prefix.into_iter().chain(text).collect()
}

/// Font size of a footnote's text at the reader's `font_size`.
pub(super) fn footnote_font_size(font_size: f32) -> f32 {
    (font_size * 0.8).max(10.0)
}

/// The prefix drawn before characters from `start` on of paragraph `index`
/// of footnote `id`: the footnote's label before its first paragraph, as an
/// ordered list numbers an item, and else an indent as wide as the label, so
/// the paragraphs line up with the text after it.
fn footnote_prefix(id: &str, index: usize, start: usize) -> String {
    // Strip any non-digit prefix, so "fn1" gives "1", "fn-2" gives "2".
    let digits = id.rfind(|c: char| !c.is_ascii_digit()).map_or(0, |i| i + 1);
    let label = if digits < id.len() { &id[digits..] } else { id };
    let label = format!("  {label}.\u{a0}");
    if index == 0 && start == 0 {
        label
    } else {
        "\u{a0}".repeat(label.chars().count())
    }
}

/// The spans characters `start..end` of paragraph `index` (`text` styled by
/// `spans`) of footnote `id` are drawn with, after the paragraph's prefix
/// (see [`footnote_prefix`]), at the reader's `font_size`.
#[allow(clippy::too_many_arguments)]
pub(super) fn footnote_paragraph_spans<'a>(
    id: &str,
    index: usize,
    text: &'a str,
    spans: &'a [TextSpan],
    start: usize,
    end: usize,
    family: font::Family,
    font_size: f32,
) -> Vec<Span<'a, EpubViewerMessage>> {
    prefixed_spans(
        Some(footnote_prefix(id, index, start)),
        text,
        spans,
        start,
        end,
        family,
        footnote_font_size(font_size),
    )
}

/// The child blocks of a block quote, footnote or figure, which draw them in
/// a column; `None` for other blocks.
pub(super) fn container_children(block: &ContentBlock) -> Option<&[ContentBlock]> {
    match block {
        ContentBlock::BlockQuote { children } => Some(children),
        ContentBlock::Footnote { blocks, .. } | ContentBlock::Figure { blocks, .. } => Some(blocks),
        _ => None,
    }
}

/// Space between the items of a list.
pub(super) fn list_item_spacing() -> f32 {
    theme::active().cosmic().spacing.space_xxs as f32
}

/// Lay out list item `spans` `width` pixels wide exactly as
/// [`render_partial_list`] draws them.
pub(super) fn layout_list_item(
    spans: &[Span<'_, EpubViewerMessage>],
    family: font::Family,
    size: f32,
    width: f32,
) -> Paragraph {
    layout_paragraph(spans, &BlockStyle::default(), family, size, width)
}

/// Draw list `items` (numbered from `ordered_start` when ordered) from `from`
/// up to `to` (`None`: to the end of the list), each item as a paragraph at
/// the reader's `font_size`.
pub(super) fn render_partial_list<'a>(
    items: &'a [ListItem],
    ordered_start: Option<u32>,
    from: BlockCut,
    to: Option<BlockCut>,
    font_size: f32,
    family: font::Family,
) -> Element<'a, EpubViewerMessage> {
    let parts = list_item_parts(items, ordered_start, from, to);
    let mut col = widget::column::with_capacity(parts.len())
        .spacing(list_item_spacing())
        .width(Length::Fill);
    for part in &parts {
        col = col.push(paragraph_text(
            list_item_part_spans(&items[part.index], part, family, font_size),
            &BlockStyle::default(),
            family,
            font_size,
        ));
    }
    col.into()
}

fn styled_span<'a>(
    ts: &'a TextSpan,
    family: font::Family,
    font_size: f32,
) -> cosmic::iced::widget::text::Span<'a, EpubViewerMessage> {
    style_span(
        span(ts.text.as_str()),
        family,
        font_size,
        &ts.style,
        ts.color,
        ts.font_size_em,
        ts.link.clone(),
    )
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
