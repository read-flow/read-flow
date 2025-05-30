// SPDX-License-Identifier: GPL-3.0-or-later
use archive_organizer::api::{File, FileDataSource};
use cosmic::iced::Length;
use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::iced_widget::list::Content;
use cosmic::widget;
use cosmic::{Apply, Element, Task};
use std::path::Path;

pub enum ArchiveStatus {
    New,
    Loading,
    Failed(String),
    Loaded(Content<File>),
}

fn view_file<'a>(file: &'a File) -> Element<'a, FilesMessage> {
    display_path(&file.path)
}

fn display_path<'a>(path: &'a str) -> Element<'a, FilesMessage> {
    let path: &Path = path.as_ref();
    let directory = format!("{}", path.parent().unwrap().display());
    let filename = path.file_name().unwrap();
    cosmic::iced_widget::column![
        widget::text(format!("{}", filename.to_string_lossy())),
        widget::text(directory).size(11),
    ]
    .spacing(5)
    .into()
}

impl ArchiveStatus {
    pub fn view(&self) -> Element<FilesMessage> {
        match self {
            ArchiveStatus::New => widget::text("New").into(),
            ArchiveStatus::Loading => widget::text("Loading").into(),
            ArchiveStatus::Failed(error) => widget::text(format!("Error: {error}")).into(),
            ArchiveStatus::Loaded(files) => {
                let list =
                    cosmic::iced::widget::list(files, |_index, file| view_file(file)).spacing(10);
                list.apply(widget::scrollable::vertical).into()
            }
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

impl<C: FileDataSource + Send + Sync + Clone + 'static> Files<C> {
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
                self.archive = ArchiveStatus::Loaded(Content::with_items(files));
                cosmic::task::none()
            }
            FilesMessage::LoadingFailed(error) => {
                self.archive = ArchiveStatus::Failed(error);
                cosmic::task::none()
            }
        }
    }
}
