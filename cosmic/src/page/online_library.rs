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
use read_flow_core::online_library::fetch_cover_bytes;

use crate::ApplicationModule;
use crate::app::ContextView;
use crate::component::pagination::Pagination;
use crate::component::pagination::PaginationMessage;
use crate::fl;
use crate::layout::layout;
use crate::page::Page;
use crate::state::LoadedState;

const DEBOUNCE_MS: u64 = 400;

// ─── State ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ResultsLayout {
    #[default]
    Cards,
    Compact,
}

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
    cover_images: HashMap<String, widget::image::Handle>,
    format_dialog: Option<OnlineBook>,
    results_layout: ResultsLayout,
    pagination: Pagination,
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
    /// Selects the catalog at index `i`; `None` means "all catalogs".
    CatalogFilterChanged(Option<usize>),
    LayoutChanged(ResultsLayout),
    Paginate(PaginationMessage),
    RequestDownload(OnlineBook),
    PickFormat(OnlineBook, DownloadFormat),
    DownloadCompleted(String, PathBuf),
    DownloadFailed(String, String),
    ImportCompleted(String),
    ImportFailed(String, String),
    CoverImageLoaded(String, Vec<u8>),
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
            cover_images: HashMap::new(),
            format_dialog: None,
            results_layout: ResultsLayout::default(),
            pagination: Pagination::default(),
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
                let visible: Vec<&OnlineBook> = self.pagination.filter_visible(books).collect();

                let items: Vec<Element<'_, OnlineLibraryMessage>> = match self.results_layout {
                    ResultsLayout::Cards => visible
                        .iter()
                        .map(|b| book_card(b, &self.download_state, self.cover_images.get(&b.id)))
                        .collect(),
                    ResultsLayout::Compact => visible
                        .iter()
                        .map(|b| book_row(b, &self.download_state))
                        .collect(),
                };

                let content: Vec<Element<'_, OnlineLibraryMessage>> =
                    vec![self.pagination.view().map(OnlineLibraryMessage::Paginate)];

                let mut content = items.into_iter().fold(content, |mut acc, item| {
                    acc.push(item);
                    acc
                });

                // content.extend(&mut items);
                content.push(self.pagination.view().map(OnlineLibraryMessage::Paginate));

                let spacing = match self.results_layout {
                    ResultsLayout::Cards => 8,
                    ResultsLayout::Compact => 2,
                };

                let list = layout(widget::column::with_children(content).spacing(spacing))
                    .apply(widget::scrollable::vertical)
                    .width(Length::Fill)
                    .height(Length::Fill);

                widget::column::with_children(vec![list.into()])
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
        // ── Catalog filter ───────────────────────────────────────────────────
        let current_idx = self.selected_catalog_index;
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
        let catalog_section = widget::settings::section()
            .title(fl!("online-library-catalog-section-title"))
            .add(widget::column::with_children(radios).spacing(8));

        // ── Layout picker ────────────────────────────────────────────────────
        let layout_section = widget::settings::section()
            .title(fl!("online-library-layout-section-title"))
            .add(
                widget::column::with_children(vec![
                    widget::radio(
                        widget::text::body(fl!("online-library-layout-cards")),
                        ResultsLayout::Cards,
                        Some(self.results_layout),
                        OnlineLibraryMessage::LayoutChanged,
                    )
                    .into(),
                    widget::radio(
                        widget::text::body(fl!("online-library-layout-compact")),
                        ResultsLayout::Compact,
                        Some(self.results_layout),
                        OnlineLibraryMessage::LayoutChanged,
                    )
                    .into(),
                ])
                .spacing(8),
            );

        ContextView {
            title: fl!("online-library-page-title"),
            content: widget::settings::view_column(vec![
                catalog_section.into(),
                layout_section.into(),
            ])
            .into(),
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
                self.cover_images.clear();
                self.debounce_counter += 1;
                self.pagination.collection_size = 0;
                self.pagination.index = 0;
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
                self.cover_images.clear();
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
                self.pagination.collection_size = books.len();
                self.pagination.index = 0;
                self.cover_images.clear();

                let cover_tasks: Vec<Task<Action<OnlineLibraryMessage>>> = books
                    .iter()
                    .filter_map(|b| {
                        let url = b.cover_url.clone()?;
                        let book_id = b.id.clone();
                        Some(task::future(async move {
                            match fetch_cover_bytes(&url).await {
                                Ok(bytes) => OnlineLibraryMessage::CoverImageLoaded(book_id, bytes),
                                Err(e) => {
                                    tracing::debug!("cover fetch failed for {book_id}: {e}");
                                    OnlineLibraryMessage::CoverImageLoaded(book_id, vec![])
                                }
                            }
                        }))
                    })
                    .collect();

                self.search_state = LoadedState::Loaded(books);
                Task::batch(cover_tasks)
            }

            OnlineLibraryMessage::CatalogFilterChanged(index) => {
                self.selected_catalog_index = index;
                if !self.search_query.is_empty() {
                    task::message(OnlineLibraryMessage::SearchStarted)
                } else {
                    Task::none()
                }
            }

            OnlineLibraryMessage::LayoutChanged(layout) => {
                self.results_layout = layout;
                Task::none()
            }

            OnlineLibraryMessage::Paginate(msg) => {
                let _ = self.pagination.update(msg);
                Task::none()
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

            OnlineLibraryMessage::CoverImageLoaded(book_id, bytes) => {
                if !bytes.is_empty() {
                    self.cover_images
                        .insert(book_id, widget::image::Handle::from_bytes(bytes));
                }
                Task::none()
            }

            OnlineLibraryMessage::Out(_) => {
                panic!("Out message should be handled by parent")
            }
        }
    }
}

// ─── Shared action area ───────────────────────────────────────────────────────

fn book_action_area<'a>(
    book: &'a OnlineBook,
    download_state: &'a HashMap<String, DownloadBookState>,
) -> Element<'a, OnlineLibraryMessage> {
    match download_state.get(&book.id) {
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
    }
}

// ─── Card layout ─────────────────────────────────────────────────────────────

fn book_card<'a>(
    book: &'a OnlineBook,
    download_state: &'a HashMap<String, DownloadBookState>,
    cover: Option<&'a widget::image::Handle>,
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

    let text_content = widget::column::with_children(vec![
        widget::row::with_children(vec![
            title.into(),
            widget::Space::new().width(Length::Fill).into(),
            catalog_badge.into(),
        ])
        .spacing(space_s)
        .into(),
        authors,
        summary,
        book_action_area(book, download_state),
    ])
    .spacing(space_xs)
    .width(Length::Fill);

    let card_body: Element<'_, OnlineLibraryMessage> = if let Some(handle) = cover {
        widget::row::with_children(vec![
            widget::image(handle.clone())
                .width(Length::Fixed(80.0))
                .height(Length::Fixed(120.0))
                .content_fit(cosmic::iced::ContentFit::Contain)
                .into(),
            text_content.into(),
        ])
        .spacing(space_m)
        .align_y(cosmic::iced::alignment::Vertical::Top)
        .into()
    } else {
        text_content.into()
    };

    card_body
        .apply(widget::container)
        .class(cosmic::theme::Container::Card)
        .padding(space_m)
        .width(Length::Fill)
        .into()
}

// ─── Compact row layout ───────────────────────────────────────────────────────

fn book_row<'a>(
    book: &'a OnlineBook,
    download_state: &'a HashMap<String, DownloadBookState>,
) -> Element<'a, OnlineLibraryMessage> {
    let cosmic_theme::Spacing {
        space_xxs,
        space_xs,
        space_s,
        ..
    } = theme::active().cosmic().spacing;

    let catalog_badge = widget::text::caption(book.catalog_name.as_str())
        .apply(widget::container)
        .class(cosmic::theme::Container::Card)
        .padding([2, space_xs]);

    let mut title_col: Vec<Element<'_, OnlineLibraryMessage>> =
        vec![widget::text::body(book.title.as_str()).into()];

    if !book.authors.is_empty() {
        title_col.push(widget::text(book.authors.join(", ")).size(11).into());
    }

    let title_column = widget::column::with_children(title_col)
        .spacing(space_xxs)
        .apply(widget::container)
        .width(Length::Fill);

    widget::row::with_children(vec![
        title_column.into(),
        catalog_badge.into(),
        book_action_area(book, download_state),
    ])
    .spacing(space_s)
    .align_y(Vertical::Center)
    .padding([space_xs, space_s])
    .apply(widget::container)
    .class(cosmic::theme::Container::Card)
    .width(Length::Fill)
    .into()
}
