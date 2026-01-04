use std::cmp;
use std::iter::Skip;
use std::iter::Take;
use std::slice::Iter;

use archive_organizer::Builder;
use cosmic::Action;
use cosmic::Apply;
use cosmic::Element;
use cosmic::Task;
use cosmic::cosmic_theme;
use cosmic::iced::Length;
use cosmic::iced::alignment::Horizontal;
use cosmic::theme;
use cosmic::widget;
use cosmic::widget::button;
use cosmic::widget::icon;

use crate::ICON_SIZE;
use crate::fl;

pub struct Pagination {
    pub page_size: usize,
    pub collection_size: usize,
    pub index: usize,
}

#[derive(Debug, Clone)]
pub enum PaginationOutput {}

#[derive(Debug, Clone)]
pub enum PaginationMessage {
    ChangePageSize(usize),
    NavigateToFirstPage,
    NavigateToPreviousPage,
    NavigateToNextPage,
    NavigateToLastPage,
    SetCollectionSize(usize),
    Out(PaginationOutput),
}

impl From<PaginationOutput> for PaginationMessage {
    fn from(value: PaginationOutput) -> Self {
        PaginationMessage::Out(value)
    }
}

impl Default for Pagination {
    fn default() -> Self {
        Self::new(0)
    }
}

impl Pagination {
    pub fn new(collection_size: usize) -> Self {
        Self {
            page_size: 10,
            collection_size,
            index: 0,
        }
    }

    pub fn filter_visible<'a, T>(&'a self, items: &'a [T]) -> Take<Skip<Iter<'a, T>>> {
        let first_visible = (self.index / self.page_size) * self.page_size;
        items.iter().skip(first_visible).take(self.page_size)
    }

    fn page(&self) -> usize {
        self.index / self.page_size + 1
    }

    fn total_pages(&self) -> usize {
        self.collection_size / self.page_size + 1
    }

    pub fn view(&self) -> Element<'_, PaginationMessage> {
        let cosmic_theme::Spacing { space_s, .. } = theme::active().cosmic().spacing;

        vec![
            button::icon(icon::from_name("go-first-symbolic").size(ICON_SIZE))
                .apply_if(self.page() > 1, |button| {
                    button.on_press(PaginationMessage::NavigateToFirstPage)
                })
                .tooltip(fl!("pagination-first"))
                .into(),
            button::icon(icon::from_name("go-previous-symbolic").size(ICON_SIZE))
                .apply_if(self.page() > 1, |button| {
                    button.on_press(PaginationMessage::NavigateToPreviousPage)
                })
                .tooltip(fl!("pagination-prev"))
                .into(),
            widget::text(fl!(
                "pagination-page-of-total",
                page = self.page(),
                total = self.total_pages()
            ))
            .align_x(Horizontal::Center)
            .width(Length::Fill)
            .into(),
            widget::dropdown(
                &["10", "20", "30", "50", "80"],
                self.page_size_to_dropdown_index(),
                PaginationMessage::ChangePageSize,
            )
            .into(),
            button::icon(icon::from_name("go-next-symbolic").size(ICON_SIZE))
                .apply_if(self.page() < self.total_pages(), |button| {
                    button.on_press(PaginationMessage::NavigateToNextPage)
                })
                .tooltip(fl!("pagination-next"))
                .into(),
            button::icon(icon::from_name("go-last-symbolic").size(ICON_SIZE))
                .apply_if(self.page() < self.total_pages(), |button| {
                    button.on_press(PaginationMessage::NavigateToLastPage)
                })
                .tooltip(fl!("pagination-last"))
                .into(),
        ]
        .apply(widget::Row::with_children)
        // .padding([0, space_s])
        .spacing(space_s)
        .into()
    }

    pub fn update(&mut self, message: PaginationMessage) -> Task<Action<PaginationMessage>> {
        match message {
            PaginationMessage::ChangePageSize(new_size) => {
                self.page_size = match new_size {
                    0 => 10,
                    1 => 20,
                    2 => 30,
                    3 => 50,
                    4 => 80,
                    _ => unreachable!(),
                };
            }
            PaginationMessage::NavigateToFirstPage => {
                self.index = 0;
            }
            PaginationMessage::NavigateToPreviousPage => {
                if self.index > self.page_size {
                    self.index -= self.page_size;
                } else {
                    self.index = 0;
                }
            }
            PaginationMessage::NavigateToNextPage => {
                self.index = cmp::min(self.index + self.page_size, self.collection_size - 1);
            }
            PaginationMessage::NavigateToLastPage => self.index = self.collection_size - 1,
            PaginationMessage::SetCollectionSize(new_size) => {
                self.collection_size = new_size;
                self.index = cmp::min(self.index, new_size);
            }
            PaginationMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent component")
            }
        }
        Task::none()
    }

    fn page_size_to_dropdown_index(&self) -> Option<usize> {
        if self.page_size <= 10 {
            Some(0)
        } else if self.page_size <= 20 {
            Some(1)
        } else if self.page_size <= 30 {
            Some(2)
        } else if self.page_size <= 50 {
            Some(3)
        } else {
            Some(4)
        }
    }
}
