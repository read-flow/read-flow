// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::Path;

use cosmic::Action;
use cosmic::Apply;
use cosmic::Element;
use cosmic::Task;
use cosmic::cosmic_theme;
use cosmic::iced::Length;
use cosmic::iced::alignment::Vertical;
use cosmic::theme;
use cosmic::widget;
use cosmic::widget::Column;
use cosmic::widget::Row;

use crate::aggregator::Document;
use crate::component::pagination::Pagination;
use crate::component::pagination::PaginationMessage;
use crate::cosmic_ext::ActionExt;
use crate::fl;
use crate::state::LoadedState;
use crate::state::filtered::Filtered;

pub type DocumentState = LoadedState<Filtered<Document>>;

#[derive(Debug, Clone)]
pub enum DocumentsOutput {
    DocumentClicked(Document),
}

#[derive(Debug, Clone)]
pub enum DocumentsMessage {
    Pagination(PaginationMessage),
    Out(DocumentsOutput),
}

impl From<PaginationMessage> for DocumentsMessage {
    fn from(value: PaginationMessage) -> Self {
        DocumentsMessage::Pagination(value)
    }
}

#[derive(Default)]
pub struct DocumentsComponent {
    pub documents: DocumentState,
    pub pagination: Pagination,
}

impl DocumentsComponent {
    pub fn view(&self) -> Element<'_, DocumentsMessage> {
        let cosmic_theme::Spacing { space_s, .. } = theme::active().cosmic().spacing;

        match &self.documents {
            DocumentState::New | DocumentState::Loading => Row::new()
                .spacing(space_s)
                .align_y(Vertical::Center)
                .push(
                    widget::icon::from_name("content-loading-symbolic")
                        .size(24)
                        .icon(),
                )
                .push(widget::text(fl!("document-list-loading")))
                .into(),
            DocumentState::Failed(error) => {
                widget::text(fl!("generic-error", error = error.as_str())).into()
            }
            DocumentState::Loaded(files) => {
                let filtered_files = files.filtered_items();
                let visible_files: Vec<_> = self
                    .pagination
                    .filter_visible(filtered_files.as_slice())
                    .collect();

                // Handle empty state
                if visible_files.is_empty() {
                    return Column::new()
                        .push(
                            widget::container(widget::text(fl!("document-list-no-files")))
                                .width(Length::Fill)
                                .center_x(Length::Fill)
                                .padding(32),
                        )
                        .into();
                }

                // Build the settings section with files
                let files_section =
                    widget::settings::section().add(self.pagination.view().map(Into::into));
                let files_section = visible_files
                    .into_iter()
                    .fold(files_section, |section, file| {
                        section.add(view_document(file))
                    })
                    .add(self.pagination.view().map(Into::into));

                let file_content = widget::settings::view_column(vec![files_section.into()])
                    .apply(widget::scrollable::vertical);

                Column::new().push(file_content).into()
            }
        }
    }

    pub fn update(&mut self, message: DocumentsMessage) -> Task<Action<DocumentsMessage>> {
        match message {
            DocumentsMessage::Pagination(message) => {
                self.pagination.update(message).map(ActionExt::map_into)
            }
            DocumentsMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent component")
            }
        }
    }

    pub fn set_filtered_indices(&mut self, files: Vec<usize>) {
        self.documents.unwrap_mut().set_filtered_indices(files);
    }
}

fn view_document<'a>(document: &'a Document) -> Element<'a, DocumentsMessage> {
    let cosmic_theme::Spacing { space_s, .. } = theme::active().cosmic().spacing;

    let icon_name = document.metadata.type_.get_file_type_icon();

    // Create a button with icon and file path that fills the width
    let button = widget::button::custom(
        Row::new()
            .push(widget::icon::from_name(icon_name).size(16).icon())
            .push(display_path(&document.local_or_any_source().path))
            .spacing(space_s)
            .align_y(cosmic::iced::Alignment::Center)
            .width(Length::Fill),
    )
    .width(Length::Fill)
    .class(widget::button::ButtonClass::Icon)
    .on_press(DocumentsMessage::Out(DocumentsOutput::DocumentClicked(
        document.clone(),
    )));

    button.into()
}

fn display_path<'a>(path: &'a str) -> Element<'a, DocumentsMessage> {
    let cosmic_theme::Spacing { space_xxs, .. } = theme::active().cosmic().spacing;

    let path: &Path = path.as_ref();
    let directory = path.parent().unwrap().display().to_string();
    let filename = path.file_name().unwrap();
    cosmic::iced_widget::column![
        widget::text(filename.display().to_string()),
        widget::text(directory).size(11),
    ]
    .spacing(space_xxs)
    .apply(widget::container)
    .width(Length::Fill)
    .into()
}
