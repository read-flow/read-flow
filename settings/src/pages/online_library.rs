use iced::Element;
use iced::Length;
use iced::widget::button;
use iced::widget::checkbox;
use iced::widget::column;
use iced::widget::row;
use iced::widget::text;
use iced::widget::text_input;
use read_flow_core::online_library::OnlineCatalog;
use read_flow_core::settings::OnlineLibrarySettings;

use crate::app::Message;
use crate::widgets::settings_section::form_card;
use crate::widgets::settings_section::settings_section;

#[derive(Debug, Clone)]
pub struct CatalogForm {
    pub original_index: Option<usize>,
    pub name: String,
    pub search_url: String,
    pub enabled: bool,
}

impl CatalogForm {
    pub fn new_empty() -> Self {
        Self {
            original_index: None,
            name: String::new(),
            search_url: String::new(),
            enabled: true,
        }
    }

    pub fn from_catalog(index: usize, catalog: &OnlineCatalog) -> Self {
        Self {
            original_index: Some(index),
            name: catalog.name.clone(),
            search_url: catalog.search_url.clone(),
            enabled: catalog.enabled,
        }
    }

    pub fn to_catalog(&self) -> OnlineCatalog {
        OnlineCatalog {
            name: self.name.clone(),
            search_url: self.search_url.clone(),
            enabled: self.enabled,
        }
    }
}

#[derive(Debug, Clone)]
pub enum CatalogFormMessage {
    NameChanged(String),
    SearchUrlChanged(String),
    EnabledToggled(bool),
}

pub fn view_online_library<'a>(
    lib: &'a OnlineLibrarySettings,
    catalog_form: Option<&'a CatalogForm>,
) -> Element<'a, Message> {
    let mut catalog_rows: Vec<Element<'a, Message>> = lib
        .catalogs
        .iter()
        .enumerate()
        .flat_map(|(i, cat)| {
            let is_editing = catalog_form
                .and_then(|f| f.original_index)
                .map(|idx| idx == i)
                .unwrap_or(false);

            let header_row: Element<'a, Message> = row![
                checkbox(cat.enabled).on_toggle(move |_| Message::CatalogToggleEnabled(i)),
                text(cat.name.clone()).width(iced::Fill),
                text(cat.search_url.clone()).size(12).width(iced::Fill),
                button(text("Edit"))
                    .style(button::secondary)
                    .on_press(Message::CatalogEditStart(i)),
                button(text("Remove"))
                    .style(button::danger)
                    .on_press(Message::CatalogRemove(i)),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center)
            .into();

            if is_editing {
                let form = catalog_form.unwrap();
                vec![header_row, view_catalog_form(form)]
            } else {
                vec![header_row]
            }
        })
        .collect();

    let adding = catalog_form
        .map(|f| f.original_index.is_none())
        .unwrap_or(false);
    if adding {
        catalog_rows.push(view_catalog_form(catalog_form.unwrap()));
    }

    catalog_rows.push(
        button(text("+ Add Catalog"))
            .style(button::secondary)
            .on_press(Message::CatalogAddStart)
            .into(),
    );

    column![
        text("Online Library").size(20),
        text("OPDS catalog feeds for searching and downloading books.").size(13),
        settings_section(Some("Catalogs"), catalog_rows),
    ]
    .spacing(12)
    .padding(20)
    .into()
}

fn view_catalog_form(form: &CatalogForm) -> Element<'_, Message> {
    let name_row = row![
        text("Name:").width(100),
        text_input("Catalog name", &form.name)
            .on_input(|s| Message::CatalogForm(CatalogFormMessage::NameChanged(s)))
            .width(Length::Fill),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center)
    .width(Length::Fill);

    let url_row = row![
        text("Search URL:").width(100),
        text_input("OPDS search URL with {searchTerms}", &form.search_url)
            .on_input(|s| Message::CatalogForm(CatalogFormMessage::SearchUrlChanged(s)))
            .width(Length::Fill),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center)
    .width(Length::Fill);

    let enabled_row = checkbox(form.enabled)
        .label("Enabled")
        .on_toggle(|b| Message::CatalogForm(CatalogFormMessage::EnabledToggled(b)));

    let buttons = row![
        button(text("Save"))
            .style(button::primary)
            .on_press(Message::CatalogSave),
        button(text("Cancel"))
            .style(button::secondary)
            .on_press(Message::CatalogCancel),
    ]
    .spacing(8);

    let inner = column![name_row, url_row, enabled_row, buttons].spacing(8);

    form_card(inner)
}
