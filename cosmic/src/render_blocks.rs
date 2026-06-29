use cosmic::Element;
use cosmic::cosmic_theme;
use cosmic::iced::Font;
use cosmic::iced::Length;
use cosmic::iced::font;
use cosmic::iced::widget::rich_text;
use cosmic::iced::widget::span;
use cosmic::theme;
use cosmic::widget;
use epub::ContentBlock;
use epub::TextSpan;

type Span<'a> = cosmic::iced::widget::text::Span<'a, ()>;

/// Render content blocks into a vertical column, generic over the app message
/// type. Links render as styled but non-clickable text — suitable for
/// display-only contexts like OPDS book descriptions.
pub fn render_blocks<'a, M: Clone + 'static>(
    blocks: &'a [ContentBlock],
    font_size: f32,
) -> Element<'a, M> {
    let cosmic_theme::Spacing { space_xxs, .. } = theme::active().cosmic().spacing;
    let mut col = widget::Column::new().spacing(space_xxs).width(Length::Fill);
    for block in blocks {
        col = col.push(render_block(block, font_size));
    }
    col.into()
}

fn render_block<'a, M: Clone + 'static>(block: &'a ContentBlock, font_size: f32) -> Element<'a, M> {
    let cosmic_theme::Spacing {
        space_xxs, space_s, ..
    } = theme::active().cosmic().spacing;

    match block {
        ContentBlock::Heading {
            level, text, spans, ..
        } => {
            if spans.is_empty() {
                match level {
                    1 => widget::text::title1(text.as_str())
                        .width(Length::Fill)
                        .into(),
                    2 => widget::text::title2(text.as_str())
                        .width(Length::Fill)
                        .into(),
                    3 => widget::text::title3(text.as_str())
                        .width(Length::Fill)
                        .into(),
                    4 => widget::text::title4(text.as_str())
                        .width(Length::Fill)
                        .into(),
                    _ => widget::text::heading(text.as_str())
                        .width(Length::Fill)
                        .into(),
                }
            } else {
                render_spans(spans, font_size, font::Family::SansSerif)
            }
        }
        ContentBlock::Paragraph { text, spans, .. } => {
            if spans.is_empty() {
                widget::text::body(text.as_str())
                    .size(font_size)
                    .width(Length::Fill)
                    .into()
            } else {
                render_spans(spans, font_size, font::Family::SansSerif)
            }
        }
        ContentBlock::UnorderedList { items } => {
            let mut col = widget::Column::new().spacing(space_xxs).width(Length::Fill);
            for item in items {
                if item.spans.is_empty() {
                    col = col.push(
                        widget::text::body(format!("  \u{2022} {}", item.text))
                            .size(font_size)
                            .width(Length::Fill),
                    );
                } else {
                    col = col.push(render_list_item(
                        "  \u{2022} ".to_string(),
                        &item.spans,
                        font_size,
                    ));
                }
            }
            col.into()
        }
        ContentBlock::OrderedList { start, items } => {
            let mut col = widget::Column::new().spacing(space_xxs).width(Length::Fill);
            for (i, item) in items.iter().enumerate() {
                let n = *start as usize + i;
                if item.spans.is_empty() {
                    col = col.push(
                        widget::text::body(format!("  {n}. {}", item.text))
                            .size(font_size)
                            .width(Length::Fill),
                    );
                } else {
                    col = col.push(render_list_item(format!("  {n}. "), &item.spans, font_size));
                }
            }
            col.into()
        }
        ContentBlock::BlockQuote { children } => {
            let mut col = widget::Column::new().spacing(space_xxs).width(Length::Fill);
            for child in children {
                col = col.push(render_block(child, font_size));
            }
            widget::container(col)
                .padding([space_xxs, space_s])
                .width(Length::Fill)
                .into()
        }
        ContentBlock::Preformatted { text, spans, .. } => {
            let inner: Element<'_, M> = if spans.is_empty() {
                widget::text::monotext(text.as_str())
                    .width(Length::Fill)
                    .into()
            } else {
                render_spans(spans, font_size, font::Family::Monospace)
            };
            widget::container(inner)
                .padding([space_xxs, space_s])
                .class(cosmic::theme::Container::Secondary)
                .width(Length::Fill)
                .into()
        }
        ContentBlock::HorizontalRule => widget::divider::horizontal::default().into(),
        // Images, figures, tables, anchors, footnotes, SVG: skip in description context.
        _ => widget::Space::new()
            .width(Length::Fixed(0.0))
            .height(Length::Fixed(0.0))
            .into(),
    }
}

// Spans use `()` as the link type — no links set, no message produced.
// `RichText<'a, ()>` implements `Widget<M>` for any M, so the element is
// compatible with the caller's message type.

fn render_spans<'a, M: Clone + 'static>(
    spans: &'a [TextSpan],
    font_size: f32,
    family: font::Family,
) -> Element<'a, M> {
    let iced_spans: Vec<Span<'a>> = spans
        .iter()
        .map(|s| simple_span(s, family, font_size))
        .collect();
    rich_text(iced_spans)
        .size(font_size)
        .width(Length::Fill)
        .into()
}

fn render_list_item<'a, M: Clone + 'static>(
    prefix: String,
    spans: &'a [TextSpan],
    font_size: f32,
) -> Element<'a, M> {
    let mut iced_spans: Vec<Span<'a>> = Vec::with_capacity(spans.len() + 1);
    let prefix_span: Span<'a> = span(prefix);
    iced_spans.push(prefix_span);
    iced_spans.extend(
        spans
            .iter()
            .map(|s| simple_span(s, font::Family::SansSerif, font_size)),
    );
    rich_text(iced_spans)
        .size(font_size)
        .width(Length::Fill)
        .into()
}

/// Styled span with `()` link type — no message, compatible with any context.
fn simple_span<'a>(ts: &'a TextSpan, family: font::Family, font_size: f32) -> Span<'a> {
    let weight = if ts.style.bold {
        font::Weight::Bold
    } else {
        font::Weight::Normal
    };
    let style = if ts.style.italic {
        font::Style::Italic
    } else {
        font::Style::Normal
    };
    let mut s: Span<'a> = span(ts.text.as_str()).font(Font {
        family,
        weight,
        style,
        ..Font::default()
    });
    if ts.style.underline {
        s = s.underline(true);
    }
    if ts.style.strikethrough {
        s = s.strikethrough(true);
    }
    if ts.style.monospaced {
        s = s.font(cosmic::font::mono());
    }
    if let Some([r, g, b]) = ts.color {
        s = s.color(cosmic::iced::Color::from_rgb8(r, g, b));
    }
    if let Some(em) = ts.font_size_em {
        s = s.size(em * font_size);
    }
    // Links (ts.link) intentionally ignored — display-only context.
    s
}
