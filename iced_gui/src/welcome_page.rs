use anyhow::Result;
use iced::{
    Task, Theme,
    widget::{button, column, container, pick_list, row, text, text_input},
};
use rfd::{AsyncFileDialog, FileHandle};

use archive_organizer::ApplicationModule;

use crate::Message as GuiMessage;

#[derive(Debug, Clone)]
pub(super) enum Message {
    SelectDirectory,
    SelectedDirectory(Option<FileHandle>),
    ScanDirectory,
    ScanComplete(Option<String>),
    EditNewRemoteUrl(String),
    AddNewRemoteUrl,
    ThemeSelected(Theme),
}

impl From<Message> for GuiMessage {
    fn from(message: Message) -> Self {
        GuiMessage::Welcome(message)
    }
}

impl TryFrom<GuiMessage> for Message {
    type Error = crate::InvalidMessage;
    fn try_from(message: GuiMessage) -> Result<Self, Self::Error> {
        if let GuiMessage::Welcome(message) = message {
            Ok(message)
        } else {
            Err(crate::InvalidMessage(message))
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct Page {
    scan_directory: Option<FileHandle>,
    application_module: ApplicationModule,
    new_remote_url: String,
    theme: Theme,
}

impl Page {
    pub fn new(application_module: ApplicationModule, theme: Theme) -> Self {
        Self {
            scan_directory: None,
            application_module,
            new_remote_url: String::new(),
            theme,
        }
    }

    pub fn init(&self) -> Task<GuiMessage> {
        Task::none()
    }

    pub fn update(&mut self, message: Message) -> iced::Task<GuiMessage> {
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
                        scan_directory(file_handle.clone(), self.application_module.clone()),
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
            Message::EditNewRemoteUrl(url) => {
                self.new_remote_url = url;
                Task::none()
            }
            Message::AddNewRemoteUrl => {
                let mut new_remote_url = Default::default();
                std::mem::swap(&mut new_remote_url, &mut self.new_remote_url);
                Task::done(GuiMessage::AddNewRemoteUrl(new_remote_url))
            }
            Message::ThemeSelected(theme) => {
                self.theme = theme.clone();
                Task::done(GuiMessage::ThemeSelected(theme))
            }
        }
    }

    pub fn view_menu(&self) -> Vec<iced::Element<'_, GuiMessage>> {
        vec![
            container(
                column![
                    text_input("Remote URL", &self.new_remote_url)
                        .width(iced::Fill)
                        .on_input(|url| Message::EditNewRemoteUrl(url).into()),
                    button("Add remote")
                        .width(iced::Fill)
                        .style(button::success)
                        .on_press(Message::AddNewRemoteUrl.into()),
                ]
                .spacing(5),
            )
            .style(container::rounded_box)
            .padding(10)
            .into(),
            container(
                column![
                    pick_list(Theme::ALL, Some(self.theme.clone()), |theme| {
                        Message::ThemeSelected(theme).into()
                    })
                    .width(iced::Fill),
                ]
                .spacing(5),
            )
            .style(container::rounded_box)
            .padding(10)
            .into(),
        ]
    }

    pub fn view(&self) -> iced::Element<'_, GuiMessage> {
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

async fn select_path() -> Option<FileHandle> {
    AsyncFileDialog::new().pick_folder().await
}

async fn scan_directory(path: FileHandle, application_module: ApplicationModule) -> Option<String> {
    match application_module.scan(path.path()) {
        Ok(()) => None,
        Err(error) => Some(error.to_string()),
    }
}
