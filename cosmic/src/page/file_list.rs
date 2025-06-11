// SPDX-License-Identifier: GPL-3.0-or-later
use crate::client::Client;
use crate::fl;
use crate::state::LoadedState;
use archive_organizer::api::{File, FileDataSource};
use cosmic::iced::Length;
use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::iced_widget::list::Content;
use cosmic::widget;
use cosmic::{Apply, Element, Task};
use std::path::Path;

type FileState = LoadedState<Content<File>>;

fn view_file<'a>(file: &'a File) -> Element<'a, FileListMessage> {
    display_path(&file.path)
        .apply(cosmic::iced_widget::button)
        .on_press(FileListMessage::Out(FileListOutput::OpenFileDetails(
            file.clone(),
        )))
        .into()
}

fn display_path<'a>(path: &'a str) -> Element<'a, FileListMessage> {
    let path: &Path = path.as_ref();
    let directory = format!("{}", path.parent().unwrap().display());
    let filename = path.file_name().unwrap();
    cosmic::iced_widget::column![
        widget::text(format!("{}", filename.to_string_lossy())),
        widget::text(directory).size(11),
    ]
    .spacing(5)
    .apply(widget::container)
    .width(Length::Fill)
    .into()
}

impl FileState {
    pub fn view(&self) -> Element<FileListMessage> {
        match self {
            FileState::New => widget::text(fl!("file-list-new")).into(), // TODO: Show spinner
            FileState::Loading => widget::text(fl!("file-list-loading")).into(), // TODO: Show spinner
            FileState::Failed(error) => {
                widget::text(fl!("generic-error", error = error.as_str())).into()
            }
            FileState::Loaded(files) => {
                let list =
                    cosmic::iced::widget::list(files, |_index, file| view_file(file)).spacing(10);
                list.apply(widget::scrollable::vertical).into()
            }
        }
    }
}

pub struct FileList {
    pub client: Client,
    pub archive: FileState,
}

#[derive(Debug, Clone)]
pub enum FileListOutput {
    OpenFileDetails(File),
}

#[derive(Debug, Clone)]
pub enum FileListMessage {
    LoadArchive,
    Loaded(Vec<File>),
    LoadingFailed(String),
    Out(FileListOutput),
}

impl FileList {
    pub fn new(client: Client) -> (Self, Task<cosmic::Action<FileListMessage>>) {
        (
            Self {
                client,
                archive: FileState::default(),
            },
            cosmic::task::message(FileListMessage::LoadArchive),
        )
    }

    pub fn display_name(&self) -> String {
        self.client.display_name()
    }

    pub fn view(&self) -> Element<FileListMessage> {
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
                .align_x(Horizontal::Left)
                .align_y(Vertical::Top),
        );

        column.into()
    }

    pub fn update(&mut self, message: FileListMessage) -> Task<cosmic::Action<FileListMessage>> {
        match message {
            FileListMessage::LoadArchive => {
                self.archive = FileState::Loading;
                let client = self.client.clone();
                cosmic::task::future(async move {
                    match client.get_files().await {
                        Ok(files) => FileListMessage::Loaded(files),
                        Err(error) => FileListMessage::LoadingFailed(format!("{error}")),
                    }
                })
            }
            FileListMessage::Loaded(files) => {
                self.archive = FileState::Loaded(Content::with_items(files));
                cosmic::task::none()
            }
            FileListMessage::LoadingFailed(error) => {
                self.archive = FileState::Failed(error);
                cosmic::task::none()
            }
            FileListMessage::Out(_) => {
                panic!("should be handled by the parent component")
            }
        }
    }
}
