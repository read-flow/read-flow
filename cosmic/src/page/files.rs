// SPDX-License-Identifier: GPL-3.0-or-later
use archive_organizer::api::FileDataSource;
use cosmic::iced::Length;
use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::widget;
use cosmic::{Apply, Element, Task};

pub struct Files<C: FileDataSource> {
    pub client: C,
}

#[derive(Debug, Clone)]
pub enum FilesMessage {}

impl<C: FileDataSource> Files<C> {
    pub fn new(client: C) -> Self {
        Self { client }
    }

    pub fn view(&self) -> Element<FilesMessage> {
        widget::text(self.client.display_name())
            .apply(widget::container)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Horizontal::Center)
            .align_y(Vertical::Center)
            .into()
    }

    pub fn update(&mut self, message: FilesMessage) -> Task<cosmic::Action<FilesMessage>> {
        todo!()
    }
}

impl<C> From<C> for Files<C>
where
    C: FileDataSource,
{
    fn from(client: C) -> Self {
        Self::new(client)
    }
}
