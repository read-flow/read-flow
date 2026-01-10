// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashSet;
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

use crate::ICON_SIZE;
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
    SelectionChanged(Vec<Document>),
}

#[derive(Debug, Clone)]
pub enum DocumentsMessage {
    Pagination(PaginationMessage),
    ToggleDocumentSelected(Document),
    ToggleAllSelected(bool),
    Out(DocumentsOutput),
    FilterSelectedDocuments,
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
    pub selected_documents: HashSet<String>,
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
                        .size(ICON_SIZE)
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
                let files_section = widget::settings::section();

                // Add select all checkbox
                let filtered_count = files.filtered_items().len();
                let selected_count = self.selected_documents.len();
                let all_selected = selected_count > 0 && selected_count >= filtered_count;

                let checkbox_label = if all_selected {
                    fl!("document-list-deselect-all")
                } else {
                    fl!("document-list-select-all")
                };
                let select_all_row = widget::row()
                    .spacing(space_s)
                    .align_y(Vertical::Center)
                    .push(
                        widget::checkbox(checkbox_label, all_selected)
                            .on_toggle(DocumentsMessage::ToggleAllSelected)
                            .width(Length::FillPortion(1)),
                    )
                    .push(
                        widget::text(fl!(
                            "document-list-selection-count",
                            selected = selected_count,
                            total = filtered_count
                        ))
                        .width(Length::FillPortion(5)),
                    );

                let files_section = files_section
                    .add(select_all_row)
                    .add(self.pagination.view().map(Into::into));

                let files_section = visible_files
                    .into_iter()
                    .fold(files_section, |section, file| {
                        let is_selected =
                            self.selected_documents.contains(&file.metadata.fingerprint);
                        section.add(view_document(file, is_selected))
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
            DocumentsMessage::ToggleDocumentSelected(document) => {
                let fingerprint = document.metadata.fingerprint.clone();
                if self.selected_documents.contains(&fingerprint) {
                    self.selected_documents.remove(&fingerprint);
                } else {
                    self.selected_documents.insert(fingerprint);
                }
                self.notify_selection_changed()
            }
            DocumentsMessage::ToggleAllSelected(all_selected) => {
                if all_selected {
                    if let DocumentState::Loaded(files) = &self.documents {
                        for file in files.filtered_items() {
                            self.selected_documents
                                .insert(file.metadata.fingerprint.clone());
                        }
                    }
                } else {
                    self.selected_documents.clear();
                }
                self.notify_selection_changed()
            }
            DocumentsMessage::FilterSelectedDocuments => {
                if let DocumentState::Loaded(files) = &self.documents {
                    let selected_document_count = self.selected_documents.len();
                    let filtered_fingerprints = files
                        .unfiltered()
                        .iter()
                        .map(|doc| doc.metadata.fingerprint.clone())
                        .collect::<HashSet<_>>();
                    self.selected_documents
                        .retain(|doc| filtered_fingerprints.contains(doc));
                    if self.selected_documents.len() != selected_document_count {
                        self.notify_selection_changed()
                    } else {
                        Task::none()
                    }
                } else {
                    Task::none()
                }
            }
            DocumentsMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent component")
            }
        }
    }

    pub fn set_filtered_indices(&mut self, files: Vec<usize>) {
        self.documents.unwrap_mut().set_filtered_indices(files);
    }

    fn notify_selection_changed(&self) -> Task<Action<DocumentsMessage>> {
        if let DocumentState::Loaded(files) = &self.documents {
            let selected_docs: Vec<Document> = files
                .unfiltered()
                .iter()
                .filter(|doc| self.selected_documents.contains(&doc.metadata.fingerprint))
                .cloned()
                .collect();
            cosmic::task::message(DocumentsMessage::Out(DocumentsOutput::SelectionChanged(
                selected_docs,
            )))
        } else {
            Task::none()
        }
    }

    pub fn get_selected_documents(&self) -> Vec<Document> {
        if let DocumentState::Loaded(files) = &self.documents {
            files
                .unfiltered()
                .iter()
                .filter(|doc| self.selected_documents.contains(&doc.metadata.fingerprint))
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    }
}

fn view_document<'a>(document: &'a Document, is_selected: bool) -> Element<'a, DocumentsMessage> {
    let cosmic_theme::Spacing { space_s, .. } = theme::active().cosmic().spacing;

    let icon_name = document.metadata.type_.get_file_type_icon();

    // Create a button with icon and file path that fills the width
    let button = widget::button::custom(
        Row::new()
            .push(
                widget::button::icon(
                    widget::icon::from_name(if is_selected {
                        "checkbox-checked-symbolic"
                    } else {
                        "checkbox-symbolic"
                    })
                    .size(ICON_SIZE),
                )
                .class(if is_selected {
                    widget::button::ButtonClass::Suggested
                } else {
                    widget::button::ButtonClass::Icon
                })
                .on_press(DocumentsMessage::ToggleDocumentSelected(document.clone())),
            )
            .push(widget::icon::from_name(icon_name).size(ICON_SIZE).icon())
            .push(display_path(&document.local_or_any_source().path))
            .spacing(space_s)
            .align_y(cosmic::iced::Alignment::Center)
            .width(Length::Fill),
    )
    .width(Length::Fill)
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
