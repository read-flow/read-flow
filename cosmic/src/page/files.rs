// SPDX-License-Identifier: GPL-3.0-or-later
use archive_organizer::api::{File, FileDataSource};
use cosmic::iced::Length;
use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::widget;
use cosmic::{Apply, Element, Task};
use tokio::time::Duration;

pub enum ArchiveStatus {
    New,
    Loading,
    Failed(String),
    Loaded(Vec<File>),
}

impl ArchiveStatus {
    pub fn view(&self) -> Element<FilesMessage> {
        match self {
            ArchiveStatus::Loaded(files) => {
		let column = files.iter()
		    .fold(widget::column(), |col, file| {
			col.push(widget::text(file.path.clone()))
		    });
		let column = column.push(widget::text("Loaded"));
		column.into()
	    }
            _ => widget::text("Loading").into(),
        }
    }
}

pub struct Files<C: FileDataSource> {
    pub client: C,
    pub archive: ArchiveStatus,
}

#[derive(Debug, Clone)]
pub enum FilesMessage {
    LoadArchive,
    Loaded(Vec<File>),
    LoadingFailed(String),
}

impl<C: FileDataSource + Send + Sync + Clone + 'static > Files<C> {
    pub fn new(client: C) -> (Self, Task<cosmic::Action<FilesMessage>>) {
        (
            Self {
                client,
                archive: ArchiveStatus::New,
            },
            cosmic::task::message(FilesMessage::LoadArchive),
        )
    }

    pub fn view(&self) -> Element<FilesMessage> {
        let column = widget::column();

        let column = column.push(
            widget::text(self.client.display_name())
                .apply(widget::container)
                .width(Length::Fill)
                .height(Length::Shrink)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Center),
        );

        let column = column.push(
            self.archive
                .view()
                .apply(widget::container)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Center),
        );

        column.into()
    }

    pub fn update(&mut self, message: FilesMessage) -> Task<cosmic::Action<FilesMessage>> {
        match message {
            FilesMessage::LoadArchive => {
                let client = self.client.clone();
                cosmic::task::future(async move {
                    match client.get_files().await {
                        Ok(files) => FilesMessage::Loaded(files),
                        Err(error) => FilesMessage::LoadingFailed(format!("{error}")),
                    }
                })
            }
            FilesMessage::Loaded(files) => {
                self.archive = ArchiveStatus::Loaded(files);
                cosmic::task::none()
            }
            FilesMessage::LoadingFailed(error) => {
                self.archive = ArchiveStatus::Failed(error);
                cosmic::task::none()
            }
        }
    }
}
