use iced::Border;
use iced::Element;
use iced::Length;
use iced::Theme;
use iced::border::Radius;

/// Wrap inline form content in a visually distinct card.
/// Uses `background.base` (window bg) to appear recessed inside
/// the `background.weak` section card that surrounds it.
pub fn form_card<'a, Msg: 'a>(content: impl Into<Element<'a, Msg>>) -> Element<'a, Msg> {
    container(content)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style {
                background: Some(palette.background.base.color.into()),
                text_color: Some(palette.background.base.text),
                border: Border {
                    radius: Radius::from(6.0),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .padding([12, 16])
        .width(Length::Fill)
        .into()
}
use iced::widget::column;
use iced::widget::container;
use iced::widget::rule;
use iced::widget::text;

/// Visual container matching libcosmic's `widget::settings::section`:
/// rounded card with subdued background, items separated by thin dividers.
pub fn settings_section<'a, Msg: 'a>(
    title: Option<&'a str>,
    items: Vec<Element<'a, Msg>>,
) -> Element<'a, Msg> {
    let mut rows: Vec<Element<'a, Msg>> = Vec::new();
    let item_count = items.len();
    for (i, item) in items.into_iter().enumerate() {
        rows.push(container(item).padding([8, 20]).width(Length::Fill).into());
        if i + 1 < item_count {
            rows.push(rule::horizontal(1).into());
        }
    }

    let card = container(column(rows))
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style {
                background: Some(palette.background.weak.color.into()),
                text_color: Some(palette.background.weak.text),
                border: Border {
                    radius: Radius::from(8.0),
                    ..Default::default()
                },
                ..Default::default()
            }
        })
        .width(Length::Fill);

    match title {
        Some(t) => column![text(t).size(13), card].spacing(6).into(),
        None => card.into(),
    }
}
