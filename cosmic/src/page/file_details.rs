use archive_organizer::api::File;
use cosmic::{
    Action, Apply, Element, Task,
    iced::{
        Length,
        alignment::{Horizontal, Vertical},
    },
    widget::text,
};

// TODO: store client in FileDetails, to allow operations on the [`file`]
pub struct FileDetails {
    id: i32,
    file: File,
}

#[derive(Debug, Clone)]
pub enum FileDetailsOutput {
    Close(i32),
}

#[derive(Debug, Clone)]
pub enum FileDetailsMessage {
    Out(FileDetailsOutput),
}

impl FileDetails {
    pub fn new(id: i32, file: File) -> (Self, Task<Action<FileDetailsMessage>>) {
        (FileDetails { id, file }, cosmic::task::none())
    }

    pub fn display_name(&self) -> String {
        self.file.path.clone()
    }

    pub fn view(&self) -> Element<FileDetailsMessage> {
        text(self.file.path.clone())
            .apply(cosmic::widget::container)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Horizontal::Center)
            .align_y(Vertical::Center)
            .into()
    }

    pub fn update(&mut self, message: FileDetailsMessage) -> Task<Action<FileDetailsMessage>> {
        match message {
            FileDetailsMessage::Out(_) => {
                panic!("should be handled by the parent component")
            }
        };
    }
}
