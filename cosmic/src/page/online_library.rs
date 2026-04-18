// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use cosmic::Action;
use cosmic::Apply;
use cosmic::Element;
use cosmic::Task;
use cosmic::cosmic_theme;
use cosmic::iced::Length;
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::alignment::Vertical;
use cosmic::task;
use cosmic::theme;
use cosmic::widget;
use read_flow_core::api::FileDataSource;
use read_flow_core::online_library::DownloadFormat;
use read_flow_core::online_library::OnlineBook;
use read_flow_core::online_library::OnlineCatalog;
use read_flow_core::online_library::OnlineLibraryClient;
use read_flow_core::online_library::OpdsClient;
use read_flow_core::online_library::download_book;

use crate::ApplicationModule;
use crate::app::ContextView;
use crate::fl;
use crate::layout::layout;
use crate::page::Page;
use crate::state::LoadedState;

const DEBOUNCE_MS: u64 = 400;

// ─── State ───────────────────────────────────────────────────────────────────

pub struct OnlineLibraryPage {
    application_module: Arc<ApplicationModule>,
    search_query: String,
    search_input_id: widget::Id,
    debounce_counter: u32,
    search_state: LoadedState<Vec<OnlineBook>>,
    /// Cached catalog list, populated after the first search.
    catalogs: Vec<OnlineCatalog>,
    /// None = all catalogs; Some(i) = catalogs[i] only.
    selected_catalog_index: Option<usize>,
    download_state: HashMap<String, DownloadBookState>,
    format_dialog: Option<OnlineBook>,
}

#[derive(Debug, Clone)]
enum DownloadBookState {
    Downloading,
    Done,
    Failed(String),
}

// ─── Messages ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum OnlineLibraryMessage {
    SearchChanged(String),
    ClearSearch,
    DebounceTimeout(u32, String),
    SearchStarted,
    SearchCompleted(Vec<OnlineBook>, Vec<OnlineCatalog>),
    SearchFailed(String),
    /// Selects the catalog at index `i`; `None` means "all catalogs".
    CatalogFilterChanged(Option<usize>),
    RequestDownload(OnlineBook),
    PickFormat(OnlineBook, DownloadFormat),
    DownloadCompleted(String, PathBuf),
    DownloadFailed(String, String),
    ImportCompleted(String),
    ImportFailed(String, String),
    DismissFormatDialog,
    Out(OnlineLibraryOutput),
}

#[derive(Debug, Clone)]
pub enum OnlineLibraryOutput {
    BookImported,
}

// ─── Page Implementation ──────────────────────────────────────────────────────

impl OnlineLibraryPage {
    pub fn new(application_module: Arc<ApplicationModule>) -> Self {
        Self {
            application_module,
            search_query: String::new(),
            search_input_id: widget::Id::unique(),
            debounce_counter: 0,
            search_state: LoadedState::New,
            catalogs: Vec::new(),
            selected_catalog_index: None,
            download_state: HashMap::new(),
            format_dialog: None,
        }
    }
}

impl Page for OnlineLibraryPage {
    type Message = OnlineLibraryMessage;

    fn view(&self) -> Element<'_, OnlineLibraryMessage> {
        let content: Element<'_, OnlineLibraryMessage> = match &self.search_state {
            LoadedState::New => widget::text(fl!("online-library-empty-state"))
                .apply(widget::container)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Center)
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),

            LoadedState::Loading => widget::text(fl!("online-library-searching"))
                .apply(widget::container)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Center)
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),

            LoadedState::Failed(err) => widget::text(err.as_str())
                .apply(widget::container)
                .align_x(Horizontal::Center)
                .align_y(Vertical::Center)
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),

            LoadedState::Loaded(books) if books.is_empty() => {
                widget::text(fl!("online-library-no-results"))
                    .apply(widget::container)
                    .align_x(Horizontal::Center)
                    .align_y(Vertical::Center)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            }

            LoadedState::Loaded(books) => {
                let items: Vec<Element<'_, OnlineLibraryMessage>> = books
                    .iter()
                    .map(|book| book_card(book, &self.download_state))
                    .collect();

                layout(widget::column::with_children(items).spacing(8))
                    .apply(widget::scrollable::vertical)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            }
        };

        widget::container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn view_header_center(&self) -> Vec<Element<'_, OnlineLibraryMessage>> {
        let search =
            widget::search_input(fl!("online-library-search-placeholder"), &self.search_query)
                .id(self.search_input_id.clone())
                .always_active()
                .on_input(OnlineLibraryMessage::SearchChanged)
                .on_clear(OnlineLibraryMessage::ClearSearch)
                .width(Length::Fixed(300.0));
        vec![search.into()]
    }

    fn view_context(&self) -> ContextView<'_, OnlineLibraryMessage> {
        let current_idx: Option<usize> = self.selected_catalog_index;

        let all_radio = widget::radio(
            widget::text::body(fl!("online-library-catalog-all")),
            None::<usize>,
            Some(current_idx),
            OnlineLibraryMessage::CatalogFilterChanged,
        );

        let mut radios: Vec<Element<'_, OnlineLibraryMessage>> = vec![all_radio.into()];

        for (i, catalog) in self.catalogs.iter().enumerate() {
            radios.push(
                widget::radio(
                    widget::text::body(catalog.name.as_str()),
                    Some(i),
                    Some(current_idx),
                    OnlineLibraryMessage::CatalogFilterChanged,
                )
                .into(),
            );
        }

        let section = widget::settings::section()
            .title(fl!("online-library-catalog-section-title"))
            .add(widget::column::with_children(radios).spacing(8));

        ContextView {
            title: fl!("online-library-page-title"),
            content: widget::settings::view_column(vec![section.into()]).into(),
        }
    }

    fn dialog(&self) -> Option<Element<'_, OnlineLibraryMessage>> {
        let cosmic_theme::Spacing { space_s, .. } = theme::active().cosmic().spacing;

        let book = self.format_dialog.as_ref()?;

        let format_buttons: Vec<Element<'_, OnlineLibraryMessage>> = book
            .formats
            .iter()
            .map(|fmt| {
                widget::button::standard(fmt.label.as_str())
                    .on_press(OnlineLibraryMessage::PickFormat(book.clone(), fmt.clone()))
                    .into()
            })
            .collect();

        let controls = widget::column::with_children(format_buttons)
            .spacing(space_s)
            .apply(widget::container)
            .class(cosmic::theme::Container::Card)
            .padding(space_s)
            .width(Length::Fill);

        Some(
            widget::dialog()
                .title(fl!("online-library-pick-format"))
                .body(book.title.as_str())
                .control(controls)
                .secondary_action(
                    widget::button::standard(fl!("online-library-cancel"))
                        .on_press(OnlineLibraryMessage::DismissFormatDialog),
                )
                .into(),
        )
    }

    fn update(&mut self, message: OnlineLibraryMessage) -> Task<Action<OnlineLibraryMessage>> {
        match message {
            OnlineLibraryMessage::SearchChanged(query) => {
                self.search_query = query.clone();
                self.debounce_counter += 1;
                let counter = self.debounce_counter;
                task::future(async move {
                    tokio::time::sleep(tokio::time::Duration::from_millis(DEBOUNCE_MS)).await;
                    OnlineLibraryMessage::DebounceTimeout(counter, query)
                })
            }

            OnlineLibraryMessage::ClearSearch => {
                self.search_query.clear();
                self.search_state = LoadedState::New;
                self.debounce_counter += 1;
                Task::none()
            }

            OnlineLibraryMessage::DebounceTimeout(counter, query) => {
                if counter == self.debounce_counter {
                    if query.is_empty() {
                        self.search_state = LoadedState::New;
                        Task::none()
                    } else {
                        task::message(OnlineLibraryMessage::SearchStarted)
                    }
                } else {
                    Task::none()
                }
            }

            OnlineLibraryMessage::SearchStarted => {
                self.search_state = LoadedState::Loading;
                let am = self.application_module.clone();
                let query = self.search_query.clone();
                let catalog_index = self.selected_catalog_index;

                task::future(async move {
                    let settings = am.settings().await;
                    let all_catalogs: Vec<OnlineCatalog> = settings
                        .online_library
                        .catalogs
                        .into_iter()
                        .filter(|c| c.enabled)
                        .collect();

                    let to_search: Vec<OnlineCatalog> = match catalog_index {
                        None => all_catalogs.clone(),
                        Some(i) => all_catalogs.get(i).cloned().into_iter().collect(),
                    };

                    let searches = to_search.into_iter().map(|catalog| {
                        let q = query.clone();
                        async move {
                            let client = OpdsClient::new(catalog);
                            match client.search(&q).await {
                                Ok(books) => books,
                                Err(e) => {
                                    tracing::warn!("OPDS search failed: {e}");
                                    vec![]
                                }
                            }
                        }
                    });

                    let results: Vec<Vec<OnlineBook>> = futures::future::join_all(searches).await;
                    let all_results: Vec<OnlineBook> = results.into_iter().flatten().collect();

                    OnlineLibraryMessage::SearchCompleted(all_results, all_catalogs)
                })
            }

            OnlineLibraryMessage::SearchCompleted(books, catalogs) => {
                self.catalogs = catalogs;
                self.search_state = LoadedState::Loaded(books);
                Task::none()
            }

            OnlineLibraryMessage::SearchFailed(err) => {
                self.search_state = LoadedState::Failed(err);
                Task::none()
            }

            OnlineLibraryMessage::CatalogFilterChanged(index) => {
                self.selected_catalog_index = index;
                if !self.search_query.is_empty() {
                    task::message(OnlineLibraryMessage::SearchStarted)
                } else {
                    Task::none()
                }
            }

            OnlineLibraryMessage::RequestDownload(book) => {
                self.format_dialog = Some(book);
                Task::none()
            }

            OnlineLibraryMessage::DismissFormatDialog => {
                self.format_dialog = None;
                Task::none()
            }

            OnlineLibraryMessage::PickFormat(book, format) => {
                self.format_dialog = None;
                let book_id = book.id.clone();
                let book_title = book.title.clone();
                self.download_state
                    .insert(book_id.clone(), DownloadBookState::Downloading);

                let am = self.application_module.clone();
                task::future(async move {
                    let settings = am.settings().await;
                    let download_folder = settings.client.download_folder.get_full_path();
                    match download_book(&format, &book_title, &download_folder).await {
                        Ok(path) => OnlineLibraryMessage::DownloadCompleted(book_id, path),
                        Err(e) => OnlineLibraryMessage::DownloadFailed(book_id, e.to_string()),
                    }
                })
            }

            OnlineLibraryMessage::DownloadCompleted(book_id, path) => {
                let am = self.application_module.clone();
                task::future(async move {
                    let db = am.db_client().await;
                    match db.import_file(&path).await {
                        Ok(_) => OnlineLibraryMessage::ImportCompleted(book_id),
                        Err(e) => OnlineLibraryMessage::ImportFailed(book_id, e.to_string()),
                    }
                })
            }

            OnlineLibraryMessage::DownloadFailed(book_id, err) => {
                tracing::warn!("download failed for {book_id}: {err}");
                self.download_state
                    .insert(book_id, DownloadBookState::Failed(err));
                Task::none()
            }

            OnlineLibraryMessage::ImportCompleted(book_id) => {
                self.download_state.insert(book_id, DownloadBookState::Done);
                task::message(OnlineLibraryMessage::Out(OnlineLibraryOutput::BookImported))
            }

            OnlineLibraryMessage::ImportFailed(book_id, err) => {
                tracing::warn!("import failed for {book_id}: {err}");
                self.download_state
                    .insert(book_id, DownloadBookState::Failed(err));
                Task::none()
            }

            OnlineLibraryMessage::Out(_) => {
                panic!("Out message should be handled by parent")
            }
        }
    }
}

// ─── Book Card Widget ─────────────────────────────────────────────────────────

fn book_card<'a>(
    book: &'a OnlineBook,
    download_state: &'a HashMap<String, DownloadBookState>,
) -> Element<'a, OnlineLibraryMessage> {
    let cosmic_theme::Spacing {
        space_xs,
        space_s,
        space_m,
        ..
    } = theme::active().cosmic().spacing;

    let title = widget::text::title4(book.title.as_str());

    let authors: Element<'_, OnlineLibraryMessage> = if book.authors.is_empty() {
        widget::Space::new().width(0).height(0).into()
    } else {
        widget::text(book.authors.join(", ")).into()
    };

    let catalog_badge = widget::text::caption(book.catalog_name.as_str())
        .apply(widget::container)
        .class(cosmic::theme::Container::Card)
        .padding([2, space_xs]);

    let summary: Element<'_, OnlineLibraryMessage> = if let Some(s) = &book.summary {
        let display = if s.len() > 200 {
            format!("{}…", &s[..200])
        } else {
            s.clone()
        };
        widget::text(display).into()
    } else {
        widget::Space::new().width(0).height(0).into()
    };

    let action_area: Element<'_, OnlineLibraryMessage> = match download_state.get(&book.id) {
        Some(DownloadBookState::Downloading) => {
            widget::text(fl!("online-library-downloading")).into()
        }
        Some(DownloadBookState::Done) => widget::text(fl!("online-library-downloaded")).into(),
        Some(DownloadBookState::Failed(err)) => widget::text(err.as_str()).into(),
        None => {
            if book.formats.len() == 1 {
                let fmt = &book.formats[0];
                widget::button::standard(fmt.label.as_str())
                    .on_press(OnlineLibraryMessage::PickFormat(book.clone(), fmt.clone()))
                    .into()
            } else {
                widget::button::suggested(fl!("online-library-download"))
                    .on_press(OnlineLibraryMessage::RequestDownload(book.clone()))
                    .into()
            }
        }
    };

    widget::column::with_children(vec![
        widget::row::with_children(vec![
            title.into(),
            widget::Space::new().width(Length::Fill).into(),
            catalog_badge.into(),
        ])
        .spacing(space_s)
        .into(),
        authors,
        summary,
        action_area,
    ])
    .spacing(space_xs)
    .padding(space_m)
    .apply(widget::container)
    .class(cosmic::theme::Container::Card)
    .width(Length::Fill)
    .into()
}
