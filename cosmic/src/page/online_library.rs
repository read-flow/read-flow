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
use cosmic::widget::Column;
use read_flow_core::online_library::DownloadFormat;
use read_flow_core::online_library::OnlineBook;
use read_flow_core::online_library::OnlineCatalog;
use read_flow_core::online_library::OpdsClient;
use read_flow_core::online_library::download_book;
use read_flow_core::online_library::fetch_cover_bytes;

use crate::ApplicationModule;
use crate::app::ContextView;
use crate::component::pagination::Pagination;
use crate::component::pagination::PaginationMessage;
use crate::component::pagination::PaginationOutput;
use crate::fl;
use crate::layout::layout;
use crate::page::Page;
use crate::render_blocks;
use crate::state::LoadedState;

// ─── State ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ResultsLayout {
    #[default]
    Cards,
    Compact,
}

/// @feature: online_library.search
pub struct OnlineLibraryPage {
    application_module: Arc<ApplicationModule>,
    search_query: String,
    search_input_id: widget::Id,
    search_state: LoadedState<Vec<OnlineBook>>,
    /// Cached catalog list, populated after the first search.
    catalogs: Vec<OnlineCatalog>,
    /// None = all catalogs; Some(i) = catalogs[i] only.
    selected_catalog_index: Option<usize>,
    download_state: HashMap<String, DownloadBookState>,
    cover_images: HashMap<String, widget::image::Handle>,
    /// catalog_name → next OPDS page URL, when the server has more pages.
    next_urls: HashMap<String, String>,
    fetching_more: bool,
    selected_book: Option<OnlineBook>,
    /// Parsed HTML blocks for `selected_book.summary_html`, cached on selection.
    selected_book_blocks: Vec<epub::ContentBlock>,
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
/// @feature: online_library.download_import
pub enum OnlineLibraryMessage {
    SearchChanged(String),
    SearchSubmitted,
    ClearSearch,
    SearchStarted,
    SearchCompleted(Vec<OnlineBook>, Vec<OnlineCatalog>, HashMap<String, String>),
    /// Selects the catalog at index `i`; `None` means "all catalogs".
    CatalogFilterChanged(Option<usize>),
    LayoutChanged(ResultsLayout),
    Paginate(PaginationMessage),
    SelectBook(OnlineBook),
    ClearSelectedBook,
    PickFormat(OnlineBook, DownloadFormat),
    DownloadCompleted(OnlineBook, PathBuf),
    DownloadFailed(String, String),
    ImportCompleted(String),
    ImportFailed(String, String),
    CoverImageLoaded(String, Vec<u8>),
    FetchMore,
    MoreResultsCompleted(Vec<OnlineBook>, HashMap<String, String>),
    Out(OnlineLibraryOutput),
}

#[derive(Debug, Clone)]
pub enum OnlineLibraryOutput {
    BookImported,
    OpenContext,
}

// ─── Page Implementation ──────────────────────────────────────────────────────

impl OnlineLibraryPage {
    pub fn new(application_module: Arc<ApplicationModule>) -> Self {
        Self {
            application_module,
            search_query: String::new(),
            search_input_id: widget::Id::unique(),
            search_state: LoadedState::New,
            catalogs: Vec::new(),
            selected_catalog_index: None,
            download_state: HashMap::new(),
            cover_images: HashMap::new(),
            next_urls: HashMap::new(),
            fetching_more: false,
            selected_book: None,
            selected_book_blocks: Vec::new(),
            results_layout: ResultsLayout::default(),
            pagination: Pagination::default(),
        }
    }

    fn search_bar(&self) -> widget::TextInput<'_, OnlineLibraryMessage> {
        widget::search_input(fl!("online-library-search-placeholder"), &self.search_query)
            .id(self.search_input_id.clone())
            .always_active()
            .on_input(OnlineLibraryMessage::SearchChanged)
            .on_submit(|_| OnlineLibraryMessage::SearchSubmitted)
            .on_clear(OnlineLibraryMessage::ClearSearch)
    }
}

impl Page for OnlineLibraryPage {
    type Message = OnlineLibraryMessage;

    fn view(&self) -> Element<'_, OnlineLibraryMessage> {
        let content: Element<'_, OnlineLibraryMessage> = match &self.search_state {
            LoadedState::Loaded(books) if !books.is_empty() => {
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

            _ => self.view_empty_state(),
        };

        widget::container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn view_header_center(&self) -> Vec<Element<'_, OnlineLibraryMessage>> {
        let show = matches!(&self.search_state, LoadedState::Loaded(books) if !books.is_empty());
        if !show {
            return vec![];
        }
        let search =
            widget::search_input(fl!("online-library-search-placeholder"), &self.search_query)
                .id(self.search_input_id.clone())
                .always_active()
                .on_clear(OnlineLibraryMessage::ClearSearch)
                .width(Length::Fixed(300.0));
        vec![search.into()]
    }

    fn view_context(&self) -> ContextView<'_, OnlineLibraryMessage> {
        match &self.selected_book {
            Some(book) => self.book_detail_context(book),
            None => self.filters_context(),
        }
    }

    fn update(&mut self, message: OnlineLibraryMessage) -> Task<Action<OnlineLibraryMessage>> {
        match message {
            OnlineLibraryMessage::SearchChanged(query) => {
                self.search_query = query;
                Task::none()
            }

            OnlineLibraryMessage::SearchSubmitted => {
                if self.search_query.is_empty() {
                    self.search_state = LoadedState::New;
                    Task::none()
                } else {
                    task::message(OnlineLibraryMessage::SearchStarted)
                }
            }

            OnlineLibraryMessage::ClearSearch => {
                self.search_query.clear();
                self.search_state = LoadedState::New;
                self.cover_images.clear();
                self.next_urls.clear();
                self.pagination.has_more = false;
                self.fetching_more = false;
                self.pagination.collection_size = 0;
                self.pagination.index = 0;
                self.selected_book = None;
                self.selected_book_blocks.clear();
                Task::none()
            }

            OnlineLibraryMessage::SearchStarted => {
                self.search_state = LoadedState::Loading;
                self.cover_images.clear();
                self.next_urls.clear();
                self.pagination.has_more = false;
                self.selected_book = None;
                self.selected_book_blocks.clear();
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
                            let catalog_name = catalog.name.clone();
                            let client = OpdsClient::new(catalog);
                            match client.search_with_next(&q).await {
                                Ok((books, next_url)) => (catalog_name, books, next_url),
                                Err(e) => {
                                    tracing::warn!("OPDS search failed: {e}");
                                    (catalog_name, vec![], None)
                                }
                            }
                        }
                    });

                    let results = futures::future::join_all(searches).await;
                    let mut all_results: Vec<OnlineBook> = Vec::new();
                    let mut next_urls: HashMap<String, String> = HashMap::new();
                    for (catalog_name, mut books, next_url) in results {
                        all_results.append(&mut books);
                        if let Some(url) = next_url {
                            next_urls.insert(catalog_name, url);
                        }
                    }

                    OnlineLibraryMessage::SearchCompleted(all_results, all_catalogs, next_urls)
                })
            }

            OnlineLibraryMessage::SearchCompleted(books, catalogs, next_urls) => {
                self.catalogs = catalogs;
                self.pagination.collection_size = books.len();
                self.pagination.index = 0;
                self.cover_images.clear();
                self.next_urls = next_urls;
                self.pagination.has_more = !self.next_urls.is_empty();
                self.fetching_more = false;

                let cover_tasks: Vec<Task<Action<OnlineLibraryMessage>>> = books
                    .iter()
                    .filter_map(|b| {
                        let url = b.cover_url.clone()?;
                        let book_id = b.id.clone();
                        Some(task::future(async move {
                            match fetch_cover_bytes(&url).await {
                                Ok((bytes, _)) => {
                                    OnlineLibraryMessage::CoverImageLoaded(book_id, bytes)
                                }
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
                if let PaginationMessage::Out(PaginationOutput::RequestMore) = &msg {
                    return task::message(OnlineLibraryMessage::FetchMore);
                }
                let _ = self.pagination.update(msg);
                Task::none()
            }

            OnlineLibraryMessage::SelectBook(book) => {
                self.selected_book_blocks = book
                    .summary_html
                    .as_deref()
                    .map(epub::parse_html_fragment)
                    .unwrap_or_default();
                self.selected_book = Some(book);
                task::message(OnlineLibraryMessage::Out(OnlineLibraryOutput::OpenContext))
            }

            OnlineLibraryMessage::ClearSelectedBook => {
                self.selected_book = None;
                self.selected_book_blocks.clear();
                Task::none()
            }

            OnlineLibraryMessage::PickFormat(book, format) => {
                let book_id = book.id.clone();
                let book_title = book.title.clone();
                self.download_state
                    .insert(book_id.clone(), DownloadBookState::Downloading);

                let am = self.application_module.clone();
                task::future(async move {
                    let settings = am.settings().await;
                    let download_folder = settings.client.download_folder.get_full_path();
                    match download_book(&format, &book_title, &download_folder).await {
                        Ok(path) => OnlineLibraryMessage::DownloadCompleted(book, path),
                        Err(e) => OnlineLibraryMessage::DownloadFailed(book_id, e.to_string()),
                    }
                })
            }

            OnlineLibraryMessage::DownloadCompleted(book, path) => {
                let book_id = book.id.clone();
                let meta = book.to_extracted_metadata();
                let cover_url = book.cover_url.clone();
                let am = self.application_module.clone();
                task::future(async move {
                    let db = am.db_client().await;
                    match db.import_with_opds_metadata(&path, &meta).await {
                        Ok(file) => {
                            if let Some(url) = cover_url {
                                match fetch_cover_bytes(&url).await {
                                    Ok((bytes, mime)) if !bytes.is_empty() => {
                                        if let Err(e) =
                                            db.store_cover(&file.fingerprint, &bytes, &mime).await
                                        {
                                            tracing::warn!(
                                                "failed to store cover for {}: {e}",
                                                file.fingerprint
                                            );
                                        }
                                    }
                                    Err(e) => {
                                        tracing::debug!("cover fetch failed on import: {e}");
                                    }
                                    _ => {}
                                }
                            }
                            OnlineLibraryMessage::ImportCompleted(book_id)
                        }
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

            OnlineLibraryMessage::FetchMore => {
                if self.next_urls.is_empty() || self.fetching_more {
                    return Task::none();
                }
                self.fetching_more = true;
                self.pagination.has_more = false;
                let am = self.application_module.clone();
                let next_urls = self.next_urls.clone();
                task::future(async move {
                    let settings = am.settings().await;
                    let catalogs: HashMap<String, OnlineCatalog> = settings
                        .online_library
                        .catalogs
                        .into_iter()
                        .map(|c| (c.name.clone(), c))
                        .collect();

                    let fetches = next_urls.into_iter().filter_map(|(catalog_name, url)| {
                        let catalog = catalogs.get(&catalog_name)?.clone();
                        Some(async move {
                            let client = OpdsClient::new(catalog);
                            match client.fetch_next_page(&url).await {
                                Ok((books, next_url)) => (catalog_name, books, next_url),
                                Err(e) => {
                                    tracing::warn!("OPDS next-page fetch failed: {e}");
                                    (catalog_name, vec![], None)
                                }
                            }
                        })
                    });

                    let results = futures::future::join_all(fetches).await;
                    let mut new_books: Vec<OnlineBook> = Vec::new();
                    let mut new_next_urls: HashMap<String, String> = HashMap::new();
                    for (catalog_name, mut books, next_url) in results {
                        new_books.append(&mut books);
                        if let Some(url) = next_url {
                            new_next_urls.insert(catalog_name, url);
                        }
                    }

                    OnlineLibraryMessage::MoreResultsCompleted(new_books, new_next_urls)
                })
            }

            OnlineLibraryMessage::MoreResultsCompleted(new_books, new_next_urls) => {
                self.fetching_more = false;
                self.next_urls = new_next_urls;
                self.pagination.has_more = !self.next_urls.is_empty();

                if let LoadedState::Loaded(books) = &mut self.search_state {
                    let old_size = books.len();

                    let cover_tasks: Vec<Task<Action<OnlineLibraryMessage>>> = new_books
                        .iter()
                        .filter_map(|b| {
                            let url = b.cover_url.clone()?;
                            let book_id = b.id.clone();
                            Some(task::future(async move {
                                match fetch_cover_bytes(&url).await {
                                    Ok((bytes, _)) => {
                                        OnlineLibraryMessage::CoverImageLoaded(book_id, bytes)
                                    }
                                    Err(e) => {
                                        tracing::debug!("cover fetch failed for {book_id}: {e}");
                                        OnlineLibraryMessage::CoverImageLoaded(book_id, vec![])
                                    }
                                }
                            }))
                        })
                        .collect();

                    books.extend(new_books);
                    let new_size = books.len();
                    self.pagination.collection_size = new_size;
                    // Navigate to the page containing the first new result.
                    self.pagination.index = old_size.min(new_size.saturating_sub(1));

                    return Task::batch(cover_tasks);
                }
                Task::none()
            }

            OnlineLibraryMessage::Out(_) => {
                panic!("Out message should be handled by parent")
            }
        }
    }
}

// ─── Context pane ─────────────────────────────────────────────────────────────

impl OnlineLibraryPage {
    fn filters_context(&self) -> ContextView<'_, OnlineLibraryMessage> {
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

    fn book_detail_context<'a>(
        &'a self,
        book: &'a OnlineBook,
    ) -> ContextView<'a, OnlineLibraryMessage> {
        let cosmic_theme::Spacing {
            space_xs,
            space_s,
            space_m,
            space_l,
            ..
        } = theme::active().cosmic().spacing;

        let back_button = widget::button::standard(fl!("online-library-back-to-filters"))
            .on_press(OnlineLibraryMessage::ClearSelectedBook);

        let cover: Element<'_, OnlineLibraryMessage> =
            if let Some(handle) = self.cover_images.get(&book.id) {
                widget::image(handle.clone())
                    .width(Length::Fixed(120.0))
                    .height(Length::Fixed(180.0))
                    .content_fit(cosmic::iced::ContentFit::Contain)
                    .apply(widget::container)
                    .center_x(Length::Fill)
                    .into()
            } else {
                widget::Space::new()
                    .width(Length::Fill)
                    .height(Length::Fixed(180.0))
                    .into()
            };

        let catalog_badge = widget::text::caption(book.catalog_name.as_str())
            .apply(widget::container)
            .class(cosmic::theme::Container::Card)
            .padding([2, space_xs]);

        let meta = Column::new()
            .spacing(space_xs)
            .push(widget::text::title3(book.title.as_str()))
            .push_maybe(if book.authors.is_empty() {
                None
            } else {
                Some(widget::text(book.authors.join(", ")))
            })
            .push(catalog_badge);

        let summary: Element<'_, OnlineLibraryMessage> = if !self.selected_book_blocks.is_empty() {
            render_blocks::render_blocks(&self.selected_book_blocks, 16.0)
        } else if let Some(s) = &book.summary {
            widget::text(s.as_str()).width(Length::Fill).into()
        } else {
            widget::text(fl!("online-library-no-description"))
                .apply(widget::container)
                .class(cosmic::theme::Container::Card)
                .into()
        };

        let formats: Element<'_, OnlineLibraryMessage> = match self.download_state.get(&book.id) {
            Some(DownloadBookState::Downloading) => {
                widget::text(fl!("online-library-downloading")).into()
            }
            Some(DownloadBookState::Done) => widget::text(fl!("online-library-downloaded")).into(),
            Some(DownloadBookState::Failed(err)) => widget::text(err.as_str()).into(),
            None => {
                let buttons: Vec<Element<'_, OnlineLibraryMessage>> = book
                    .formats
                    .iter()
                    .map(|fmt| {
                        widget::button::suggested(fmt.label.as_str())
                            .on_press(OnlineLibraryMessage::PickFormat(book.clone(), fmt.clone()))
                            .width(Length::Fill)
                            .into()
                    })
                    .collect();
                Column::new().spacing(space_s).extend(buttons).into()
            }
        };

        let content = Column::new()
            .spacing(space_m)
            .push(back_button)
            .push(cover)
            .push(meta)
            .push(widget::divider::horizontal::default())
            .push(summary)
            .push(widget::divider::horizontal::default())
            .push(formats)
            .padding([0, space_s, space_l, space_s])
            .apply(widget::scrollable::vertical)
            .height(Length::Fill);

        ContextView {
            title: fl!("online-library-book-details"),
            content: content.into(),
        }
    }
}

// ─── Empty state ──────────────────────────────────────────────────────────────

impl OnlineLibraryPage {
    fn view_empty_state(&self) -> Element<'_, OnlineLibraryMessage> {
        let cosmic_theme::Spacing {
            space_s,
            space_m,
            space_l,
            space_xl,
            ..
        } = theme::active().cosmic().spacing;

        let is_new = matches!(self.search_state, LoadedState::New);

        let hero_section = Column::new()
            .spacing(space_m)
            .align_x(Horizontal::Center)
            .push(
                widget::icon::from_name("accessories-dictionary-symbolic")
                    .size(80)
                    .icon(),
            )
            .push(widget::text::title2(fl!("online-library-welcome-title")))
            .push(widget::text(fl!("online-library-welcome-subtitle")).width(Length::Fixed(460.0)));

        let search_row = widget::row::with_children(vec![
            self.search_bar().width(Length::Fill).into(),
            widget::button::suggested(fl!("online-library-search-button"))
                .on_press(OnlineLibraryMessage::SearchSubmitted)
                .into(),
        ])
        .spacing(space_s)
        .align_y(Vertical::Center)
        .width(Length::Fixed(480.0));

        let hints = widget::row::with_children(vec![
            hint_card(
                "system-search-symbolic",
                fl!("online-library-hint-search-title"),
                fl!("online-library-hint-search-body"),
                space_s,
                space_m,
            ),
            hint_card(
                "folder-download-symbolic",
                fl!("online-library-hint-download-title"),
                fl!("online-library-hint-download-body"),
                space_s,
                space_m,
            ),
            hint_card(
                "user-bookmarks-symbolic",
                fl!("online-library-hint-library-title"),
                fl!("online-library-hint-library-body"),
                space_s,
                space_m,
            ),
        ])
        .spacing(space_m)
        .width(Length::Fill);

        let mut below_search = Column::new().spacing(space_m).width(Length::Fill);
        if !is_new {
            let status_text = match &self.search_state {
                LoadedState::Loading => fl!("online-library-searching"),
                LoadedState::Loaded(_) => fl!("online-library-no-results"),
                LoadedState::Failed(err) => err.clone(),
                LoadedState::New => unreachable!(),
            };
            below_search = below_search.push(
                widget::text(status_text)
                    .apply(widget::container)
                    .center_x(Length::Fill),
            );
        }
        let below_search: Element<'_, OnlineLibraryMessage> = below_search.push(hints).into();

        widget::container(
            widget::container(
                Column::new()
                    .spacing(space_l)
                    .align_x(Horizontal::Center)
                    .push(hero_section)
                    .push(search_row)
                    .push(below_search),
            )
            .max_width(700.0),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding(space_xl)
        .into()
    }
}

fn hint_card<'a>(
    icon_name: &'static str,
    title: String,
    body: String,
    space_s: u16,
    space_m: u16,
) -> Element<'a, OnlineLibraryMessage> {
    Column::new()
        .spacing(space_s)
        .align_x(Horizontal::Center)
        .push(widget::icon::from_name(icon_name).size(32).icon())
        .push(widget::text::heading(title))
        .push(widget::text(body).width(Length::Fill))
        .width(Length::Fill)
        .apply(widget::container)
        .class(cosmic::theme::Container::Card)
        .padding(space_m)
        .width(Length::Fill)
        .into()
}

// ─── Download status (inline in card/row) ────────────────────────────────────

fn book_download_status<'a>(
    book: &'a OnlineBook,
    download_state: &'a HashMap<String, DownloadBookState>,
) -> Option<Element<'a, OnlineLibraryMessage>> {
    match download_state.get(&book.id) {
        Some(DownloadBookState::Downloading) => {
            Some(widget::text(fl!("online-library-downloading")).into())
        }
        Some(DownloadBookState::Done) => {
            Some(widget::text(fl!("online-library-downloaded")).into())
        }
        Some(DownloadBookState::Failed(err)) => Some(widget::text(err.as_str()).into()),
        None => None,
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

    let mut text_rows: Vec<Element<'_, OnlineLibraryMessage>> = vec![
        widget::row::with_children(vec![
            title.into(),
            widget::Space::new().width(Length::Fill).into(),
            catalog_badge.into(),
        ])
        .spacing(space_s)
        .into(),
        authors,
        summary,
    ];
    if let Some(status) = book_download_status(book, download_state) {
        text_rows.push(status);
    }

    let text_content = widget::column::with_children(text_rows)
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

    widget::mouse_area(
        card_body
            .apply(widget::container)
            .class(cosmic::theme::Container::Card)
            .padding(space_m)
            .width(Length::Fill),
    )
    .on_press(OnlineLibraryMessage::SelectBook(book.clone()))
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

    let mut row_children: Vec<Element<'_, OnlineLibraryMessage>> =
        vec![title_column.into(), catalog_badge.into()];
    if let Some(status) = book_download_status(book, download_state) {
        row_children.push(status);
    }

    widget::mouse_area(
        widget::row::with_children(row_children)
            .spacing(space_s)
            .align_y(Vertical::Center)
            .padding([space_xs, space_s])
            .apply(widget::container)
            .class(cosmic::theme::Container::Card)
            .width(Length::Fill),
    )
    .on_press(OnlineLibraryMessage::SelectBook(book.clone()))
    .into()
}
