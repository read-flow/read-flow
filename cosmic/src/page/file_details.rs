use crate::client::Client;
use archive_organizer::api::File;
use cosmic::iced_widget::Column;
use cosmic::iced_widget::Row;
use cosmic::{
    Action, Apply, Element, Task,
    iced::{
        Length,
        alignment::{Horizontal, Vertical},
    },
    widget::text,
};
use std::borrow::Cow;
use std::path::Path;

pub struct FileDetails {
    id: i32,
    file: File,
    client: Client,
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
    pub fn new(id: i32, file: File, client: Client) -> (Self, Task<Action<FileDetailsMessage>>) {
        (FileDetails { id, file, client }, cosmic::task::none())
    }

    pub fn display_name(&self) -> String {
        self.file.path.clone()
    }

    pub fn view(&self) -> Element<FileDetailsMessage> {
        // Extract filename and folder using std::path
        let path = Path::new(&self.file.path);

        // Get filename without extension
        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Unknown");

        let filename_without_extension = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or(filename);

        // Get folder path
        let folder = path
            .parent()
            .and_then(|parent| parent.to_str())
            .unwrap_or("");

        // Prepare content
        let title_row = Row::with_children(vec![
            text(filename_without_extension)
                .size(24)
                .width(Length::Fill)
                .into(),
            cosmic::widget::button::icon(cosmic::widget::icon::from_name("window-close-symbolic"))
                .on_press(FileDetailsMessage::Out(FileDetailsOutput::Close(self.id)))
                .into(),
        ]);

        let title = title_row.width(Length::Fill);

        let details = Column::with_children(vec![
            // Basic info section
            text("Basic Information").size(20).into(),
            cosmic::widget::container(Column::with_children(vec![
                row_with_label("Folder", folder),
                row_with_label("Filename", filename),
                row_with_label("Type", &self.file.type_),
                row_with_label("Size", format!("{} bytes", self.file.size)),
                row_with_label("Status", format!("{}", self.file.status)),
            ]))
            .padding(10)
            .into(),
            // Technical details section
            text("Technical Details").size(20).into(),
            cosmic::widget::container(Column::with_children(vec![
                row_with_label("ID", format!("{}", self.file.id)),
                row_with_label("Full Path", &self.file.path),
                row_with_label("Fingerprint", &self.file.fingerprint),
            ]))
            .padding(10)
            .into(),
            // Tags section
            text("Tags").size(20).into(),
            cosmic::widget::container(tags_view(&self.file.tags))
                .padding(10)
                .into(),
        ]);

        let content = Column::with_children(vec![title.into(), details.into()])
            .spacing(20)
            .padding(20);

        // Wrap content in a container
        content
            .apply(cosmic::widget::container)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Horizontal::Center)
            .align_y(Vertical::Top)
            .into()
    }

    pub fn update(&mut self, message: FileDetailsMessage) -> Task<Action<FileDetailsMessage>> {
        match message {
            FileDetailsMessage::Out(_) => {
                panic!("should be handled by the parent component")
            }
        }
    }
}

/// Helper function to create a row with a label and value
fn row_with_label<'a>(
    label: &'a str,
    value: impl Into<Cow<'a, str>> + 'a,
) -> Element<'a, FileDetailsMessage> {
    Row::with_children(vec![
        text(label).width(Length::FillPortion(1)).into(),
        text(value).width(Length::FillPortion(3)).into(),
    ])
    .spacing(10)
    .padding(5)
    .into()
}

/// Helper function to create a view for tags
fn tags_view(tags: &[String]) -> Element<FileDetailsMessage> {
    if tags.is_empty() {
        return text("No tags").into();
    }

    let tag_elements: Vec<Element<_>> = tags
        .iter()
        .map(|tag| cosmic::widget::button::text(tag).padding([5, 10]).into())
        .collect();

    Row::with_children(tag_elements).spacing(10).into()
}
