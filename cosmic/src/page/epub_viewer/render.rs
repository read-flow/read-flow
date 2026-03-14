use cosmic::Apply;
use cosmic::Element;
use cosmic::cosmic_theme;
use cosmic::iced::Background;
use cosmic::iced::Border;
use cosmic::iced::Color;
use cosmic::iced::Font;
use cosmic::iced::Length;
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::font;
use cosmic::iced::widget::rich_text;
use cosmic::iced::widget::span;
use cosmic::theme;
use cosmic::theme::Container;
use cosmic::widget;
use epub::BlockStyle;
use epub::ContentBlock;
use epub::Document as EpubDocumentTrait;
use epub::EpubDocument;
use epub::TableCell;
use epub::TextAlign;
use epub::TextSpan;

use crate::fl;
use crate::page::epub_viewer::BlockHighlight;
use crate::page::epub_viewer::EpubViewerMessage;

#[derive(Clone, Copy)]
pub(super) struct RenderContext<'a> {
    pub font_size: f32,
    pub family: font::Family,
    pub chapter_href: &'a str,
    pub epub_document: Option<&'a EpubDocument>,
    pub max_image_height: f32,
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
    let size = style
        .font_size_em
        .map(|em| em * font_size)
        .unwrap_or(font_size);
    let align = text_align_horizontal(style);
    let font = Font {
        family,
        ..Font::default()
    };
    let inner: Element<'a, EpubViewerMessage> = if spans.is_empty() {
        apply_text_align(
            widget::text::body(text)
                .size(font_size)
                .font(font)
                .width(Length::Fill)
                .align_x(align)
                .into(),
            style,
        )
    } else {
        let iced_spans: Vec<_> = spans
            .into_iter()
            .map(|s| owned_styled_span(s, family))
            .collect();
        apply_text_align(
            rich_text(iced_spans).size(size).width(Length::Fill).into(),
            style,
        )
    };
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
        let inner = self.render_block_inner(block);
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

    fn render_block_inner(&self, block: &'a ContentBlock) -> Element<'a, EpubViewerMessage> {
        let font_size = self.font_size;
        let family = self.family;
        let chapter_href = self.chapter_href;
        let epub_document = self.epub_document;
        let max_image_height = self.max_image_height;

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
            ContentBlock::Preformatted { text, spans, .. } => {
                let inner: Element<'a, EpubViewerMessage> = if spans.is_empty() {
                    widget::text::monotext(text).width(Length::Fill).into()
                } else {
                    render_spans(spans, font_size, font::Family::Monospace)
                };
                let code_block = widget::container(inner)
                    .padding([space_xxs, space_s])
                    .class(Container::Secondary)
                    .width(Length::Fill);
                let copy_button = widget::row::with_children(vec![
                    widget::space::horizontal().width(Length::Fill).into(),
                    widget::button::icon(widget::icon::from_name("edit-copy-symbolic").size(16))
                        .on_press(EpubViewerMessage::CopyCodeBlock(text.clone()))
                        .tooltip(fl!("epub-viewer-copy-code"))
                        .into(),
                ]);
                cosmic::iced::widget::stack(vec![code_block.into(), copy_button.into()]).into()
            }
            ContentBlock::BlockQuote { children } => {
                let mut col = widget::column::with_capacity(children.len())
                    .spacing(space_xxs)
                    .width(Length::Fill);
                for child in children {
                    col = col.push(self.render_block_inner(child));
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
                    .max_height(max_image_height)
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
                    widget::Space::new().width(Length::Fill).height(0).into()
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
                            Err(e) => {
                                tracing::info!(
                                    "SVG image resource not found in chapter {chapter_href}: {img_path} ({e})"
                                );
                                None
                            }
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
                    .max_height(max_image_height)
                    .align_x(Horizontal::Center)
                    .into()
            }
            ContentBlock::Table { rows } => render_table(rows, font_size, family),
            ContentBlock::HorizontalRule => widget::divider::horizontal::default().into(),
            ContentBlock::Footnote { blocks, .. } => self.render_footnote(blocks),
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
                    col = col.push(self.render_block_inner(block));
                }
                if !caption.is_empty() {
                    let iced_spans: Vec<_> =
                        caption.iter().map(|s| styled_span(s, family)).collect();
                    let caption_el: Element<'_, EpubViewerMessage> =
                        widget::container(rich_text(iced_spans).size(caption_size))
                            .width(Length::Fill)
                            .align_x(Horizontal::Center)
                            .into();
                    col = col.push(caption_el);
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
            ContentBlock::Anchor { .. } => widget::Space::new()
                .width(Length::Fixed(0.0))
                .height(Length::Fixed(0.0))
                .into(),
        }
    }

    fn render_footnote(&self, blocks: &'a [ContentBlock]) -> Element<'a, EpubViewerMessage> {
        let cosmic_theme::Spacing {
            space_xxs, space_s, ..
        } = theme::active().cosmic().spacing;

        let caption_size = (self.font_size * 0.8).max(10.0);
        let inner_ctx = RenderContext {
            max_image_height: f32::MAX,
            ..*self
        };

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
                        render_spans(spans, caption_size, self.family)
                    }
                }
                _ => inner_ctx.render_block_inner(block),
            };
            col = col.push(el);
        }

        widget::container(col)
            .padding([space_xxs, space_s])
            .class(Container::Secondary)
            .width(Length::Fill)
            .into()
    }
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
                widget::Space::new().width(Length::Fill).height(0).into()
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

/// Build an iced `Span` from an owned `TextSpan`, producing a `'static` element
/// that does not borrow the source span data.
fn owned_styled_span(
    text_span: TextSpan,
    family: font::Family,
) -> cosmic::iced::widget::text::Span<'static, EpubViewerMessage> {
    let style = text_span.style;
    let link = text_span.link;
    let color = text_span.color;
    let font_size_em = text_span.font_size_em;
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
    let mut s = span(text_span.text); // String → Cow::Owned → 'static
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
    if let Some([r, g, b]) = color {
        s = s.color(cosmic::iced::Color::from_rgb8(r, g, b));
    }
    if let Some(em) = font_size_em {
        s = s.size(em * 16.0);
    }
    if let Some(href) = link {
        s = s.link(EpubViewerMessage::FollowLink(href));
        if color.is_none() {
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
