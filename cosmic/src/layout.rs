use cosmic::Element;
use cosmic::iced::Length;
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::alignment::Vertical;
use cosmic::widget;

const MAX_CONTENT_WIDTH: f32 = 800.0;

pub fn layout<'a, E, M>(element: E) -> Element<'a, M>
where
    E: Into<Element<'a, M>>,
    M: 'a,
{
    // Inner container: caps content width at MAX_CONTENT_WIDTH, fills on narrow screens.
    // Outer container: fills full width and centers the (possibly narrower) inner container.
    widget::container(
        widget::container(element)
            .max_width(MAX_CONTENT_WIDTH)
            .width(Length::Fill)
            .height(Length::Shrink)
            .align_y(Vertical::Top),
    )
    .width(Length::Fill)
    .align_x(Horizontal::Center)
    .height(Length::Shrink)
    .into()
}

pub fn full_page<'a, E, M>(element: E) -> Element<'a, M>
where
    E: Into<Element<'a, M>>,
    M: 'a,
{
    widget::container(element)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Horizontal::Center)
        .align_y(Vertical::Center)
        .into()
}
