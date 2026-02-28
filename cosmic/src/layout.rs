use cosmic::Apply;
use cosmic::Element;
use cosmic::iced::Length;
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::alignment::Vertical;
use cosmic::widget;
use cosmic::widget::Row;

pub fn layout<'a, E, M>(element: E) -> Row<'a, M>
where
    E: Into<Element<'a, M>>,
    M: 'a,
{
    vec![
        widget::horizontal_space().into(),
        element
            .apply(widget::container)
            .width(Length::FillPortion(4))
            .height(Length::Shrink)
            .align_x(Horizontal::Center)
            .align_y(Vertical::Top)
            .into(),
        widget::horizontal_space().into(),
    ]
    .apply(Row::with_children)
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
