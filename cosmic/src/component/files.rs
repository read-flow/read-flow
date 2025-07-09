// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::Path;

use cosmic::Action;
use cosmic::Apply;
use cosmic::Element;
use cosmic::iced::Length;
use cosmic::widget;
use cosmic::widget::Row;
use cosmic::{cosmic_theme, theme};

use archive_organizer::api::File;
use cosmic::Task;

use crate::fl;
use crate::state::files::FileState;

#[derive(Debug, Clone)]
pub enum FilesOutput {
    FileClicked(File),
}

#[derive(Debug, Clone)]
pub enum FilesMessage {
    Out(FilesOutput),
}

#[derive(Default)]
pub struct FilesComponent {
    pub files: FileState,
}

impl FilesComponent {
    pub fn view(&self) -> Element<FilesMessage> {
        let cosmic_theme::Spacing { space_s, .. } = theme::active().cosmic().spacing;

        match &self.files {
            FileState::New => widget::text(fl!("file-list-new")).into(), // TODO: Show spinner
            FileState::Loading => widget::text(fl!("file-list-loading")).into(), // TODO: Show spinner
            FileState::Failed(error) => {
                widget::text(fl!("generic-error", error = error.as_str())).into()
            }
            FileState::Loaded(files) => {
                let list = cosmic::iced::widget::list(&files.visible_files, |_index, file| {
                    view_file(file)
                })
                .spacing(space_s);
                list.apply(widget::scrollable::vertical).into()
            }
        }
    }

    pub fn update(&mut self, message: FilesMessage) -> Task<Action<FilesMessage>> {
        match message {
            FilesMessage::Out(_) => {
                panic!("should be handled by the parent component")
            }
        }
    }

    pub fn set_visible(&mut self, files: Vec<File>) {
        self.files.unwrap_mut().set_visible(files);
    }
}

fn view_file<'a>(file: &'a File) -> Element<'a, FilesMessage> {
    let cosmic_theme::Spacing { space_s, .. } = theme::active().cosmic().spacing;

    let button = widget::button::custom(
        Row::new()
            .push(
                widget::icon::from_name("x-office-document-symbolic")
                    .size(16)
                    .icon(),
            )
            .push(display_path(&file.path))
            .padding([0, space_s])
            .spacing(space_s)
            .align_y(cosmic::iced::Alignment::Center),
    )
    .on_press(FilesMessage::Out(FilesOutput::FileClicked(file.clone())));

    button.into()
}

fn display_path<'a>(path: &'a str) -> Element<'a, FilesMessage> {
    let cosmic_theme::Spacing { space_xxs, .. } = theme::active().cosmic().spacing;

    let path: &Path = path.as_ref();
    let directory = format!("{}", path.parent().unwrap().display());
    let filename = path.file_name().unwrap();
    cosmic::iced_widget::column![
        widget::text(format!("{}", filename.to_string_lossy())),
        widget::text(directory).size(11),
    ]
    .spacing(space_xxs)
    .apply(widget::container)
    .width(Length::Fill)
    .into()
}
