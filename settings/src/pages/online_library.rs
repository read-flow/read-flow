use iced::Element;
use iced::widget::button;
use iced::widget::checkbox;
use iced::widget::column;
use iced::widget::row;
use iced::widget::rule;
use iced::widget::text;
use iced::widget::text_input;
use read_flow_core::online_library::OnlineCatalog;
use read_flow_core::settings::OnlineLibrarySettings;

use crate::app::Message;

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
    let mut rows: Vec<Element<'a, Message>> = lib
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
        rows.push(view_catalog_form(catalog_form.unwrap()));
    }

    rows.push(
        button(text("+ Add Catalog"))
            .style(button::secondary)
            .on_press(Message::CatalogAddStart)
            .into(),
    );

    let list = column(rows).spacing(6);

    column![
        text("Online Library").size(20),
        text("OPDS catalog feeds for searching and downloading books.").size(13),
        rule::horizontal(1),
        list,
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
            .width(250),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    let url_row = row![
        text("Search URL:").width(100),
        text_input("OPDS search URL with {searchTerms}", &form.search_url)
            .on_input(|s| Message::CatalogForm(CatalogFormMessage::SearchUrlChanged(s)))
            .width(350),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

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

    column![name_row, url_row, enabled_row, buttons]
        .spacing(8)
        .padding([8, 16])
        .into()
}
