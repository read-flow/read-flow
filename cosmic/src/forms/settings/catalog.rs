// SPDX-License-Identifier: AGPL-3.0-or-later

use cosmic::Action;
use cosmic::Element;
use cosmic::Task;
use cosmic::iced::Length;
use cosmic::task;
use cosmic::widget;
use read_flow_core::Builder;

use crate::ICON_SIZE;
use crate::fl;

/// @feature: online_library.manage_catalogs
pub struct CatalogForm {
    pub(crate) original_name: Option<String>,
    editing_name: String,
    editing_search_url: String,
}

#[derive(Debug, Clone)]
pub enum CatalogFormMessage {
    EditName(String),
    EditSearchUrl(String),
    Out(CatalogFormOutput),
}

#[derive(Debug, Clone)]
pub enum CatalogFormOutput {
    /// original_name, name, search_url
    Submit(Option<String>, String, String),
    Cancel,
}

impl From<CatalogFormOutput> for CatalogFormMessage {
    fn from(value: CatalogFormOutput) -> Self {
        Self::Out(value)
    }
}

impl CatalogForm {
    pub fn new(existing: Option<(String, String)>) -> (Self, Task<Action<CatalogFormMessage>>) {
        let (original_name, editing_name, editing_search_url) = match existing {
            Some((name, search_url)) => (Some(name.clone()), name, search_url),
            None => (None, String::new(), String::new()),
        };
        (
            Self {
                original_name,
                editing_name,
                editing_search_url,
            },
            task::none(),
        )
    }

    /// `other_names` are every other catalog's resolved display name (built-in
    /// and configured, excluding the entry currently being edited) — a name
    /// collision would let two catalogs silently share pagination state.
    fn is_submittable(&self, other_names: &[String]) -> bool {
        !self.editing_name.is_empty()
            && !self.editing_search_url.is_empty()
            && !other_names.contains(&self.editing_name)
    }

    pub fn view(&self, other_names: &[String]) -> Element<'_, CatalogFormMessage> {
        let is_adding = self.original_name.is_none();
        widget::settings::section()
            .title(if is_adding {
                fl!("settings-online-library-add-catalog-title")
            } else {
                fl!("settings-online-library-edit-catalog")
            })
            .add(
                widget::settings::item::builder(fl!("settings-online-library-catalog-name"))
                    .icon(widget::icon::from_name("text-x-generic-symbolic").size(ICON_SIZE))
                    .control(
                        widget::text_input(
                            fl!("settings-online-library-catalog-name-placeholder"),
                            &self.editing_name,
                        )
                        .on_input(CatalogFormMessage::EditName)
                        .width(Length::FillPortion(1)),
                    ),
            )
            .add(
                widget::settings::item::builder(fl!("settings-online-library-catalog-search-url"))
                    .icon(widget::icon::from_name("web-browser-symbolic").size(ICON_SIZE))
                    .control(
                        widget::text_input(
                            fl!("settings-online-library-catalog-search-url-placeholder"),
                            &self.editing_search_url,
                        )
                        .on_input(CatalogFormMessage::EditSearchUrl)
                        .width(Length::FillPortion(1)),
                    ),
            )
            .add(widget::settings::item_row(vec![
                widget::space::horizontal().width(Length::Fill).into(),
                // Cancel button
                widget::button::icon(
                    widget::icon::from_name("edit-clear-all-symbolic").size(ICON_SIZE),
                )
                .on_press(CatalogFormOutput::Cancel.into())
                .into(),
                // Submit button
                widget::button::icon(
                    widget::icon::from_name(if is_adding {
                        "list-add-symbolic"
                    } else {
                        "edit-symbolic"
                    })
                    .size(ICON_SIZE),
                )
                .class(widget::button::ButtonClass::Suggested)
                .apply_if(self.is_submittable(other_names), |button| {
                    button.on_press(
                        CatalogFormOutput::Submit(
                            self.original_name.clone(),
                            self.editing_name.clone(),
                            self.editing_search_url.clone(),
                        )
                        .into(),
                    )
                })
                .into(),
            ]))
            .into()
    }

    pub fn update(&mut self, message: CatalogFormMessage) -> Task<Action<CatalogFormMessage>> {
        match message {
            CatalogFormMessage::EditName(name) => {
                self.editing_name = name;
                task::none()
            }
            CatalogFormMessage::EditSearchUrl(search_url) => {
                self.editing_search_url = search_url;
                task::none()
            }
            CatalogFormMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent component")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(name: &str, search_url: &str) -> CatalogForm {
        CatalogForm {
            original_name: None,
            editing_name: name.to_string(),
            editing_search_url: search_url.to_string(),
        }
    }

    #[test]
    fn not_submittable_when_name_empty() {
        assert!(!form("", "https://example.com/opds").is_submittable(&[]));
    }

    #[test]
    fn not_submittable_when_search_url_empty() {
        assert!(!form("My Library", "").is_submittable(&[]));
    }

    #[test]
    fn not_submittable_when_name_collides_with_another_catalog() {
        let other_names = vec!["Project Gutenberg".to_string()];
        assert!(
            !form("Project Gutenberg", "https://example.com/opds").is_submittable(&other_names)
        );
    }

    #[test]
    fn submittable_with_unique_non_empty_name_and_url() {
        let other_names = vec!["Project Gutenberg".to_string()];
        assert!(form("My Library", "https://example.com/opds").is_submittable(&other_names));
    }

    #[test]
    fn editing_own_original_name_stays_submittable() {
        // `other_names` is expected to exclude the entry being edited, so its
        // own (unchanged) name never appears there.
        let other_names = vec!["Project Gutenberg".to_string()];
        assert!(form("Standard Ebooks", "https://example.com/opds").is_submittable(&other_names));
    }
}
