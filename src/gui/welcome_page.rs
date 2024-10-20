use anyhow::Result;
use iced::{
    widget::{button, column, container, row, text, text_input},
    Task,
};
use rfd::{AsyncFileDialog, FileHandle};

use crate::{db::ConnectionPool, gui, scan::scan};

#[derive(Debug, Clone)]
pub(super) enum Message {
    SelectDirectory,
    SelectedDirectory(Option<FileHandle>),
    ScanDirectory,
    ScanComplete(Option<String>),
}

impl From<Message> for gui::Message {
    fn from(message: Message) -> Self {
        gui::Message::Welcome(message)
    }
}

impl TryFrom<gui::Message> for Message {
    type Error = gui::InvalidMessage;
    fn try_from(message: gui::Message) -> Result<Self, Self::Error> {
        if let gui::Message::Welcome(message) = message {
            Ok(message)
        } else {
            Err(gui::InvalidMessage(message))
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct Page {
    scan_directory: Option<FileHandle>,
    connection_pool: ConnectionPool,
}

impl Page {
    pub fn new(connection_pool: ConnectionPool) -> Self {
        Self {
            scan_directory: None,
            connection_pool,
        }
    }

    pub fn init(&self) -> Task<gui::Message> {
        Task::none()
    }

    pub fn update(&mut self, message: Message) -> iced::Task<gui::Message> {
        match message {
            Message::SelectDirectory => Task::perform(select_path(), |result| {
                Message::SelectedDirectory(result).into()
            }),
            Message::SelectedDirectory(directory) => {
                self.scan_directory = directory;
                Task::none()
            }
            Message::ScanDirectory => {
                if let Some(file_handle) = &self.scan_directory {
                    Task::perform(
                        scan_directory(file_handle.clone(), self.connection_pool.clone()),
                        |result| Message::ScanComplete(result).into(),
                    )
                } else {
                    Task::none()
                }
            }
            Message::ScanComplete(None) => {
                tracing::debug!("Scan completed successfully");
                self.scan_directory = None;
                Task::none()
            }
            Message::ScanComplete(Some(error)) => {
                tracing::error!("Scan completed with error: `{error}`");
                Task::none()
            }
        }
    }

    pub fn view(&self) -> iced::Element<gui::Message> {
        container(
            column![
                text("Welcome"),
                row![
                    text_input(
                        "Directory to Scan",
                        self.scan_directory
                            .as_ref()
                            .and_then(|dir| dir.path().to_str())
                            .unwrap_or("")
                    )
                    .width(300),
                    button("Select").on_press(Message::SelectDirectory.into()),
                ],
                match &self.scan_directory {
                    Some(_) => button("Scan").on_press(Message::ScanDirectory.into()),
                    None => button("Scan"),
                }
            ]
            .spacing(10),
        )
        .into()
    }
}

impl From<Page> for gui::Pages {
    fn from(value: Page) -> Self {
        gui::Pages::Welcome(value)
    }
}

async fn select_path() -> Option<FileHandle> {
    AsyncFileDialog::new().pick_folder().await
}

async fn scan_directory(path: FileHandle, connection_pool: ConnectionPool) -> Option<String> {
    match scan(path.path().to_path_buf(), connection_pool) {
        Ok(()) => None,
        Err(error) => Some(error.to_string()),
    }
}
