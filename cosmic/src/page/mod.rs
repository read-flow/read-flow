// SPDX-License-Identifier: GPL-3.0-or-later
// pages
mod dashboard;
mod document_details;
mod document_list;
mod epub_viewer;
pub mod image_viewer;
mod mu_pdf_viewer;
mod online_library;
mod preferences;
mod traits;

use core::panic;
use std::sync::Arc;

use cosmic::Action;
use cosmic::Apply;
use cosmic::Element;
use cosmic::Task;
use cosmic::iced::Subscription;
use cosmic::iced::core::SmolStr;
use cosmic::iced::keyboard::Key;
use cosmic::iced::keyboard::Modifiers;
use cosmic::task;
use cosmic::widget;
pub use dashboard::DashboardMessage;
use dashboard::DashboardOutput;
use dashboard::DashboardPage;
use indexmap::IndexMap;
pub use online_library::OnlineLibraryMessage;
use online_library::OnlineLibraryOutput;
pub use online_library::OnlineLibraryPage;
pub use preferences::PreferencesMessage;
pub use preferences::PreferencesOutput;
pub use preferences::PreferencesPage;
use read_flow_core::api::ReadingStatus;
use read_flow_core::client::FilesClient;
pub use traits::Page;
use url::Url;

use crate::ApplicationModule;
use crate::aggregator::Aggregator;
use crate::aggregator::Document;
use crate::aggregator::DocumentType;
use crate::app::ContextView;
use crate::client::ClientSelector;
use crate::config::Config;
use crate::config::EpubViewerConfig;
use crate::cosmic_ext::ActionExt;
use crate::document_provider::DocumentProvider;
use crate::fl;
use crate::layout::full_page;
use crate::page::document_details::DocumentDetails;
use crate::page::document_details::DocumentDetailsMessage;
use crate::page::document_details::DocumentDetailsOutput;
use crate::page::document_list::DocumentList;
pub use crate::page::document_list::DocumentListMessage;
use crate::page::document_list::DocumentListOutput;
use crate::page::epub_viewer::EpubViewer;
use crate::page::epub_viewer::EpubViewerMessage;
use crate::page::epub_viewer::EpubViewerOutput;
use crate::page::image_viewer::ImageViewer;
use crate::page::image_viewer::ImageViewerMessage;
use crate::page::image_viewer::ImageViewerOutput;
pub use crate::page::image_viewer::ViewerImage;
use crate::page::mu_pdf_viewer::MuPdfViewer;
use crate::page::mu_pdf_viewer::MuPdfViewerMessage;
use crate::page::mu_pdf_viewer::MuPdfViewerOutput;
use crate::subscription::SubscriberState;

type Fingerprint = String;

pub struct PageInfo {
    pub icon_name: &'static str,
    pub label: String,
    pub parent: Option<PageSelector>,
}

pub struct Pages {
    pub(crate) document_provider: Arc<DocumentProvider>,
    application_module: Arc<ApplicationModule>,

    epub_viewer_config: EpubViewerConfig,
    dashboard: DashboardPage,
    preferences: PreferencesPage,
    online_library: OnlineLibraryPage,
    documents: DocumentList,
    document_details: IndexMap<Fingerprint, DocumentDetails>,
    epub_viewers: IndexMap<Fingerprint, EpubViewer>,
    mu_pdf_viewers: IndexMap<Fingerprint, MuPdfViewer>,
    image_viewers: IndexMap<u64, ImageViewer>,
    next_image_viewer_id: u64,
    active: PageSelector,
    page_order: IndexMap<PageSelector, PageInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PageSelector {
    Dashboard,
    Preferences,
    OnlineLibrary,
    Documents,
    DocumentDetails(Fingerprint),
    EpubViewer(Fingerprint),
    MuPdfViewer(Fingerprint),
    ImageViewer(u64),
}

#[derive(Debug, Clone)]
pub enum PageOutput {
    PageAdded(PageSelector),
    TogglePage(PageSelector),
    PageRemoved(PageSelector),
    Scan,
}

#[derive(Debug, Clone)]
pub enum PageMessage {
    Dashboard(DashboardMessage),
    Preferences(PreferencesMessage),
    OnlineLibrary(OnlineLibraryMessage),
    AddRemote(Url, String, String),
    EditRemote(Url, Url, String, String),
    DeleteRemote(Url),
    Documents(DocumentListMessage),
    DocumentDetails(Fingerprint, DocumentDetailsMessage),
    OpenDocumentDetails(Document),
    CloseDocumentDetails(Fingerprint),
    EpubViewer(Fingerprint, EpubViewerMessage),
    OpenDocument(Document),
    CloseEpubViewer(Fingerprint, Option<(String, f64)>),
    MuPdfViewer(Fingerprint, MuPdfViewerMessage),
    CloseMuPdfViewer(Fingerprint, Option<(usize, usize)>),
    ImageViewer(u64, ImageViewerMessage),
    OpenImageViewer(ViewerImage),
    CloseImageViewer(u64),
    KeyEvent(PageSelector, Modifiers, Key, Option<SmolStr>),
    ModifiersChanged(PageSelector, Modifiers),
    NavigateToDocumentsWithStatus(ReadingStatus),
    NavigateToDocumentsWithType(DocumentType),
    Refresh,
    Noop,
    Out(PageOutput),
}

impl From<OnlineLibraryMessage> for PageMessage {
    fn from(msg: OnlineLibraryMessage) -> Self {
        Self::OnlineLibrary(msg)
    }
}

impl From<DocumentListMessage> for PageMessage {
    fn from(source: DocumentListMessage) -> Self {
        Self::Documents(source)
    }
}

impl From<PreferencesMessage> for PageMessage {
    fn from(source: PreferencesMessage) -> Self {
        Self::Preferences(source)
    }
}

macro_rules! with_active_page {
    ($self:expr, $selector:expr, |$page:ident, $mapper:ident| $body:expr) => {
        match $selector {
            PageSelector::Dashboard => {
                let $page = Some(&$self.dashboard);
                let $mapper = map_dashboard_message;
                $body
            }
            PageSelector::Preferences => {
                let $page = Some(&$self.preferences);
                let $mapper = map_preferences_message;
                $body
            }
            PageSelector::OnlineLibrary => {
                let $page = Some(&$self.online_library);
                let $mapper = map_online_library_message;
                $body
            }
            PageSelector::Documents => {
                let $page = Some(&$self.documents);
                let $mapper = map_document_list_message;
                $body
            }
            PageSelector::DocumentDetails(fingerprint) => {
                let $page = $self.document_details.get(fingerprint);
                let fingerprint = fingerprint.clone();
                let $mapper = move |msg| map_document_details_message(fingerprint.clone(), msg);
                $body
            }
            PageSelector::EpubViewer(fingerprint) => {
                let $page = $self.epub_viewers.get(fingerprint);
                let fingerprint = fingerprint.clone();
                let $mapper = move |msg| map_epub_viewer_message(fingerprint.clone(), msg);
                $body
            }
            PageSelector::MuPdfViewer(fingerprint) => {
                let $page = $self.mu_pdf_viewers.get(fingerprint);
                let fingerprint = fingerprint.clone();
                let $mapper = move |msg| map_mu_pdf_viewer_message(fingerprint.clone(), msg);
                $body
            }
            PageSelector::ImageViewer(id) => {
                let $page = $self.image_viewers.get(id);
                let id = *id;
                let $mapper = move |msg| map_image_viewer_message(id, msg);
                $body
            }
        }
    };
}

fn page_not_found<'a, M: 'a>() -> Element<'a, M> {
    widget::text::title1(fl!("page-not-found")).apply(full_page)
}

impl Pages {
    pub fn new(
        application_module: Arc<ApplicationModule>,
        config: Config,
    ) -> (Self, Task<Action<PageMessage>>) {
        let epub_viewer_config = config.epub_viewer;

        let clients = vec![application_module.clone().into()];

        let document_provider = Arc::new(DocumentProvider::new(Aggregator::new(
            clients,
            application_module.clone(),
        )));

        let (preferences, init_preferences) = PreferencesPage::new(
            application_module.clone(),
            config,
            document_provider.clone(),
        );

        let (documents, init_documents) = DocumentList::new(document_provider.clone());
        let (dashboard, init_dashboard) = DashboardPage::new(document_provider.clone());

        let tasks = vec![
            init_preferences.map(ActionExt::map_into),
            init_documents.map(ActionExt::map_into),
            init_dashboard.map(|action| action.map(map_dashboard_message)),
        ];

        let online_library = OnlineLibraryPage::new(application_module.clone());

        (
            Self {
                document_provider,
                application_module,
                epub_viewer_config,
                dashboard,
                preferences,
                online_library,
                documents,
                document_details: Default::default(),
                epub_viewers: Default::default(),
                mu_pdf_viewers: Default::default(),
                image_viewers: Default::default(),
                next_image_viewer_id: 0,
                active: PageSelector::Dashboard,
                page_order: Default::default(),
            },
            task::batch(tasks),
        )
    }

    pub fn update_app_config(&mut self, config: &Config) {
        self.epub_viewer_config = config.epub_viewer;
        self.preferences.update_config(config.clone());
    }

    pub fn active_page(&self) -> &PageSelector {
        &self.active
    }

    pub fn activate(&mut self, selector: PageSelector) {
        self.active = selector;
    }

    pub fn page_list(&self) -> &IndexMap<PageSelector, PageInfo> {
        &self.page_order
    }

    pub fn register_page(
        &mut self,
        selector: PageSelector,
        icon_name: &'static str,
        label: String,
        parent: Option<PageSelector>,
    ) {
        self.page_order.insert(
            selector,
            PageInfo {
                icon_name,
                label,
                parent,
            },
        );
    }

    fn deactivate_page(&mut self, selector: &PageSelector) {
        if let Some(info) = self.page_order.swap_remove(selector)
            && self.active == *selector
        {
            self.active = info
                .parent
                .filter(|p| self.page_order.contains_key(p))
                .unwrap_or(PageSelector::Dashboard);
        }
    }

    pub fn nav_tree(
        &self,
        selector: &PageSelector,
        is_active: bool,
    ) -> Option<read_flow_widgets::NavItem<PageMessage>> {
        with_active_page!(self, selector, |page, mapper| {
            page.and_then(|p| p.nav_tree(is_active).map(|item| item.map(&mapper)))
        })
    }

    pub fn display_name<'a>(&'a self, page_selector: &'a PageSelector) -> String {
        match &page_selector {
            PageSelector::Dashboard => fl!("dashboard-page-title"),
            PageSelector::Preferences => fl!("preferences-page-title"),
            PageSelector::OnlineLibrary => fl!("online-library-page-title"),
            PageSelector::Documents => "Documents".to_string(),
            PageSelector::DocumentDetails(fingerprint) => self
                .document_details
                .get(fingerprint)
                .map(|p| p.display_name())
                .unwrap_or_default(),
            PageSelector::EpubViewer(fingerprint) => self
                .epub_viewers
                .get(fingerprint)
                .map(|p| p.display_name())
                .unwrap_or_default(),
            PageSelector::MuPdfViewer(fingerprint) => self
                .mu_pdf_viewers
                .get(fingerprint)
                .map(|p| p.display_name())
                .unwrap_or_default(),
            PageSelector::ImageViewer(id) => self
                .image_viewers
                .get(id)
                .map(|p| p.display_name())
                .unwrap_or_default(),
        }
    }

    pub fn dialog<'a>(&'a self, active_page: &'a PageSelector) -> Option<Element<'a, PageMessage>> {
        with_active_page!(self, active_page, |page, mapper| {
            page.and_then(|p| p.dialog().map(|e| e.map(mapper)))
        })
    }

    pub fn view<'a>(&'a self, active_page: &'a PageSelector) -> Element<'a, PageMessage> {
        with_active_page!(self, active_page, |page, mapper| {
            page.map(|p| p.view().map(mapper))
                .unwrap_or_else(page_not_found)
        })
    }

    #[allow(clippy::clone_on_copy)]
    pub fn view_header_start<'a>(
        &'a self,
        active_page: &'a PageSelector,
    ) -> Vec<Element<'a, PageMessage>> {
        with_active_page!(self, active_page, |page, mapper| {
            page.map(move |p| {
                p.view_header_start()
                    .into_iter()
                    .map(|e| e.map(mapper.clone()))
                    .collect()
            })
            .unwrap_or_default()
        })
    }

    #[allow(clippy::clone_on_copy)]
    pub fn view_header_center<'a>(
        &'a self,
        active_page: &'a PageSelector,
    ) -> Vec<Element<'a, PageMessage>> {
        with_active_page!(self, active_page, |page, mapper| {
            page.map(move |p| {
                p.view_header_center()
                    .into_iter()
                    .map(|e| e.map(mapper.clone()))
                    .collect()
            })
            .unwrap_or_default()
        })
    }

    #[allow(clippy::clone_on_copy)]
    pub fn view_header_end<'a>(
        &'a self,
        active_page: &'a PageSelector,
    ) -> Vec<Element<'a, PageMessage>> {
        with_active_page!(self, active_page, |page, mapper| {
            page.map(move |p| {
                p.view_header_end()
                    .into_iter()
                    .map(|e| e.map(mapper.clone()))
                    .collect()
            })
            .unwrap_or_default()
        })
    }

    pub fn view_context<'a>(
        &'a self,
        active_page: &'a PageSelector,
    ) -> ContextView<'a, PageMessage> {
        with_active_page!(self, active_page, |page, mapper| {
            page.map(|p| p.view_context().map(mapper))
                .unwrap_or_else(|| ContextView {
                    title: fl!("page-not-found"),
                    content: page_not_found(),
                })
        })
    }

    pub fn update(&mut self, message: PageMessage) -> Task<Action<PageMessage>> {
        tracing::debug!("received: {message:?}");
        match message {
            PageMessage::Noop => Task::none(),
            PageMessage::NavigateToDocumentsWithStatus(status) => {
                let filter_task = self
                    .documents
                    .update(DocumentListMessage::StatusFilterChanged(Some(status)))
                    .map(|action| action.map(map_document_list_message));
                let nav_task = task::message(PageMessage::Out(PageOutput::TogglePage(
                    PageSelector::Documents,
                )));
                Task::batch([filter_task, nav_task])
            }
            PageMessage::NavigateToDocumentsWithType(type_) => {
                let filter_task = self
                    .documents
                    .update(DocumentListMessage::TypeFilterChanged(Some(type_)))
                    .map(|action| action.map(map_document_list_message));
                let nav_task = task::message(PageMessage::Out(PageOutput::TogglePage(
                    PageSelector::Documents,
                )));
                Task::batch([filter_task, nav_task])
            }
            PageMessage::Dashboard(msg) => self
                .dashboard
                .update(msg)
                .map(|action| action.map(map_dashboard_message)),
            PageMessage::Refresh => {
                let mut messages = self
                    .document_details
                    .iter()
                    .map(|(fingerprint, _)| {
                        task::message(PageMessage::DocumentDetails(
                            fingerprint.clone(),
                            DocumentDetailsMessage::RefreshDocument,
                        ))
                    })
                    .collect::<Vec<_>>();
                messages.push(task::message(PageMessage::from(
                    DocumentListMessage::LoadArchive,
                )));
                messages.push(task::message(PageMessage::Dashboard(
                    DashboardMessage::LoadDashboard,
                )));
                Task::batch(messages)
            }
            PageMessage::DocumentDetails(fingerprint, message) => self.document_details
                [&fingerprint]
                .update(message)
                .map(move |action| {
                    action.map(|msg| map_document_details_message(fingerprint.clone(), msg))
                }),
            PageMessage::AddRemote(url, user_id, passphrase) => {
                let private_mode = self.preferences.current_private_mode();
                let document_provider = self.document_provider.clone();
                task::future(async move {
                    tracing::debug!("adding remote client: {url}");
                    document_provider
                        .add_client(
                            FilesClient::new(url, user_id, passphrase, private_mode)
                                .unwrap()
                                .into(),
                        )
                        .await;
                    PageMessage::Noop
                })
            }
            PageMessage::EditRemote(old_url, new_url, user_id, passphrase) => {
                let selector = ClientSelector::Remote(old_url);
                let private_mode = self.preferences.current_private_mode();
                let document_provider = self.document_provider.clone();
                task::future(async move {
                    document_provider.remove_client(&selector).await;
                    document_provider
                        .add_client(
                            FilesClient::new(new_url, user_id, passphrase, private_mode)
                                .unwrap()
                                .into(),
                        )
                        .await;
                    PageMessage::Noop
                })
            }
            PageMessage::DeleteRemote(url) => {
                let selector = ClientSelector::Remote(url.clone());
                let document_provider = self.document_provider.clone();
                task::future(async move {
                    tracing::debug!("removing remote client: {url}");
                    document_provider.remove_client(&selector).await;
                    PageMessage::Noop
                })
            }
            PageMessage::Preferences(preferences_message) => self
                .preferences
                .update(preferences_message)
                .map(move |action| action.map(map_preferences_message)),
            PageMessage::OnlineLibrary(msg) => self
                .online_library
                .update(msg)
                .map(move |action| action.map(map_online_library_message)),
            PageMessage::Documents(document_list_message) => self
                .documents
                .update(document_list_message)
                .map(move |action| action.map(map_document_list_message)),
            PageMessage::CloseDocumentDetails(fingerprint) => {
                let selector = PageSelector::DocumentDetails(fingerprint.clone());
                let _ = self.document_details.swap_remove(&fingerprint);
                self.deactivate_page(&selector);
                task::message(PageMessage::Out(PageOutput::PageRemoved(selector)))
            }
            PageMessage::OpenDocumentDetails(document) => {
                let fingerprint = document.document_guid.clone();
                let document_icon = document
                    .contents
                    .first()
                    .map(|c| c.type_.get_file_type_icon())
                    .unwrap_or("text-x-generic-symbolic");

                if self.document_details.contains_key(&fingerprint) {
                    task::message(PageMessage::Out(PageOutput::TogglePage(
                        PageSelector::DocumentDetails(fingerprint),
                    )))
                } else {
                    let parent = self.active.clone();
                    let fingerprint_1 = fingerprint.clone();
                    let (document_details, initialization) = DocumentDetails::new(
                        document,
                        self.documents.document_provider.clone(),
                        self.application_module.clone(),
                    );
                    self.document_details
                        .insert(fingerprint.clone(), document_details);
                    let selector = PageSelector::DocumentDetails(fingerprint);
                    let label = self.display_name(&selector);
                    self.register_page(selector.clone(), document_icon, label, Some(parent));
                    self.active = selector.clone();
                    initialization
                        .map(move |action| {
                            let fingerprint = fingerprint_1.clone();
                            action.map(move |msg| map_document_details_message(fingerprint, msg))
                        })
                        .chain(task::message(PageMessage::Out(PageOutput::PageAdded(
                            selector,
                        ))))
                }
            }
            PageMessage::KeyEvent(page, modifiers, key, text) => match page {
                PageSelector::EpubViewer(fingerprint) => {
                    let Some(viewer) = self.epub_viewers.get_mut(&fingerprint) else {
                        return Task::none();
                    };
                    viewer
                        .update(EpubViewerMessage::Key(modifiers, key, text))
                        .map(move |action| {
                            action.map(|msg| map_epub_viewer_message(fingerprint.clone(), msg))
                        })
                }
                PageSelector::MuPdfViewer(fingerprint) => {
                    let Some(viewer) = self.mu_pdf_viewers.get_mut(&fingerprint) else {
                        return Task::none();
                    };
                    viewer
                        .update(MuPdfViewerMessage::Key(modifiers, key, text))
                        .map(move |action| {
                            action.map(|msg| map_mu_pdf_viewer_message(fingerprint.clone(), msg))
                        })
                }
                PageSelector::Documents => self
                    .documents
                    .update(DocumentListMessage::Key(modifiers, key))
                    .map(|action| action.map(map_document_list_message)),
                _ => Task::none(),
            },
            PageMessage::ModifiersChanged(page, modifiers) => match page {
                PageSelector::EpubViewer(fingerprint) => {
                    let Some(viewer) = self.epub_viewers.get_mut(&fingerprint) else {
                        return Task::none();
                    };
                    viewer
                        .update(EpubViewerMessage::ModifiersChanged(modifiers))
                        .map(move |action| {
                            action.map(|msg| map_epub_viewer_message(fingerprint.clone(), msg))
                        })
                }
                PageSelector::MuPdfViewer(fingerprint) => {
                    let Some(viewer) = self.mu_pdf_viewers.get_mut(&fingerprint) else {
                        return Task::none();
                    };
                    viewer
                        .update(MuPdfViewerMessage::ModifiersChanged(modifiers))
                        .map(move |action| {
                            action.map(|msg| map_mu_pdf_viewer_message(fingerprint.clone(), msg))
                        })
                }
                _ => Task::none(),
            },
            PageMessage::MuPdfViewer(fingerprint, message) => {
                let Some(viewer) = self.mu_pdf_viewers.get_mut(&fingerprint) else {
                    return Task::none();
                };
                viewer.update(message).map(move |action| {
                    action.map(|msg| map_mu_pdf_viewer_message(fingerprint.clone(), msg))
                })
            }
            PageMessage::CloseMuPdfViewer(fingerprint, page_info) => {
                let selector = PageSelector::MuPdfViewer(fingerprint.clone());
                let _ = self.mu_pdf_viewers.swap_remove(&fingerprint);
                self.deactivate_page(&selector);

                let mut tasks = vec![task::message(PageMessage::Out(PageOutput::PageRemoved(
                    selector,
                )))];

                if let Some((page, total)) = page_info {
                    let document_provider = self.document_provider.clone();
                    let fp = fingerprint;
                    tasks.push(task::future(async move {
                        let now = iso8601_now();
                        let percentage = if total > 0 {
                            (page as f64 + 1.0) / total as f64
                        } else {
                            0.0
                        };
                        let state = read_flow_core::api::ReadingState {
                            fingerprint: fp,
                            status: 0,
                            position: format!("{{\"page\":{page}}}"),
                            percentage,
                            last_updated: now.clone(),
                            status_updated_at: "1970-01-01T00:00:00Z".to_string(),
                        };
                        if let Err(e) = document_provider.upsert_reading_state(state).await {
                            tracing::warn!("failed to save reading state: {e}");
                        }
                        PageMessage::Noop
                    }));
                }

                Task::batch(tasks)
            }
            PageMessage::EpubViewer(fingerprint, message) => {
                let Some(viewer) = self.epub_viewers.get_mut(&fingerprint) else {
                    return Task::none();
                };
                viewer.update(message).map(move |action| {
                    action.map(|msg| map_epub_viewer_message(fingerprint.clone(), msg))
                })
            }
            PageMessage::ImageViewer(id, message) => {
                let Some(viewer) = self.image_viewers.get_mut(&id) else {
                    return Task::none();
                };
                viewer
                    .update(message)
                    .map(move |action| action.map(|msg| map_image_viewer_message(id, msg)))
            }
            PageMessage::OpenImageViewer(image) => {
                let id = self.next_image_viewer_id;
                self.next_image_viewer_id += 1;
                let viewer = ImageViewer::new(id, image);
                self.image_viewers.insert(id, viewer);
                let parent = self.active.clone();
                let selector = PageSelector::ImageViewer(id);
                let label = self.display_name(&selector);
                self.register_page(
                    selector.clone(),
                    "image-x-generic-symbolic",
                    label,
                    Some(parent),
                );
                self.active = selector.clone();
                task::message(PageMessage::Out(PageOutput::PageAdded(selector)))
            }
            PageMessage::CloseImageViewer(id) => {
                let selector = PageSelector::ImageViewer(id);
                let _ = self.image_viewers.swap_remove(&id);
                self.deactivate_page(&selector);
                task::message(PageMessage::Out(PageOutput::PageRemoved(selector)))
            }
            PageMessage::OpenDocument(document) => {
                let type_ = document
                    .contents
                    .first()
                    .map(|c| c.type_)
                    .unwrap_or(DocumentType::Other);
                match type_ {
                    DocumentType::Epub => match self.epub_viewer_config {
                        EpubViewerConfig::NativeEpub => self.open_epub_viewer(document),
                        EpubViewerConfig::MuPdf => self.open_mupdf_viewer(document),
                        EpubViewerConfig::ExternalViewer => self.open_in_external_viewer(document),
                    },
                    DocumentType::Other => self.open_in_external_viewer(document),
                    _ => self.open_mupdf_viewer(document),
                }
            }
            PageMessage::CloseEpubViewer(fingerprint, progress_info) => {
                let selector = PageSelector::EpubViewer(fingerprint.clone());
                let _ = self.epub_viewers.swap_remove(&fingerprint);
                self.deactivate_page(&selector);

                let mut tasks = vec![task::message(PageMessage::Out(PageOutput::PageRemoved(
                    selector,
                )))];

                if let Some((position_json, percentage)) = progress_info {
                    let document_provider = self.document_provider.clone();
                    let fp = fingerprint;
                    tasks.push(task::future(async move {
                        let now = iso8601_now();
                        let state = read_flow_core::api::ReadingState {
                            fingerprint: fp,
                            status: 0,
                            position: position_json,
                            percentage,
                            last_updated: now,
                            status_updated_at: "1970-01-01T00:00:00Z".to_string(),
                        };
                        if let Err(e) = document_provider.upsert_reading_state(state).await {
                            tracing::warn!("failed to save reading state: {e}");
                        }
                        PageMessage::Noop
                    }));
                }

                Task::batch(tasks)
            }
            PageMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent component")
            }
        }
    }

    fn open_in_external_viewer(&mut self, document: Document) -> Task<Action<PageMessage>> {
        let document_provider = self.document_provider.clone();
        task::future(async move {
            if let Err(e) = document_provider.open_document(document).await {
                tracing::error!("Failed to open file: {e}");
            }
            PageMessage::Noop
        })
    }

    fn open_mupdf_viewer(&mut self, document: Document) -> Task<Action<PageMessage>> {
        let fingerprint = document
            .contents
            .first()
            .map(|c| c.fingerprint.clone())
            .unwrap_or_default();

        if self.mu_pdf_viewers.contains_key(&fingerprint) {
            task::message(PageMessage::Out(PageOutput::TogglePage(
                PageSelector::MuPdfViewer(fingerprint),
            )))
        } else {
            let parent = self.active.clone();
            let fingerprint_1 = fingerprint.clone();
            let (pdf_viewer, initialization) =
                MuPdfViewer::new(document, self.document_provider.clone());
            self.mu_pdf_viewers.insert(fingerprint.clone(), pdf_viewer);
            let selector = PageSelector::MuPdfViewer(fingerprint);
            let label = self.display_name(&selector);
            self.register_page(
                selector.clone(),
                "application-pdf-symbolic",
                label,
                Some(parent),
            );
            self.active = selector.clone();
            Task::batch([
                initialization.map(move |action| {
                    let fingerprint = fingerprint_1.clone();
                    action.map(move |msg| map_mu_pdf_viewer_message(fingerprint, msg))
                }),
                task::message(PageMessage::Out(PageOutput::PageAdded(selector))),
            ])
        }
    }

    fn open_epub_viewer(&mut self, document: Document) -> Task<Action<PageMessage>> {
        let fingerprint = document
            .contents
            .first()
            .map(|c| c.fingerprint.clone())
            .unwrap_or_default();

        if self.epub_viewers.contains_key(&fingerprint) {
            task::message(PageMessage::Out(PageOutput::TogglePage(
                PageSelector::EpubViewer(fingerprint),
            )))
        } else {
            let parent = self.active.clone();
            let fingerprint_1 = fingerprint.clone();
            let (epub_viewer, initialization) =
                EpubViewer::new(document, self.document_provider.clone());
            self.epub_viewers.insert(fingerprint.clone(), epub_viewer);
            let selector = PageSelector::EpubViewer(fingerprint);
            let label = self.display_name(&selector);
            self.register_page(
                selector.clone(),
                "application-epub+zip",
                label,
                Some(parent),
            );
            self.active = selector.clone();
            Task::batch([
                initialization.map(move |action| {
                    let fingerprint = fingerprint_1.clone();
                    action.map(move |msg| map_epub_viewer_message(fingerprint, msg))
                }),
                task::message(PageMessage::Out(PageOutput::PageAdded(selector))),
            ])
        }
    }
}

fn map_dashboard_message(msg: DashboardMessage) -> PageMessage {
    match msg {
        DashboardMessage::Out(output) => match output {
            DashboardOutput::NavigateToDocuments => {
                PageMessage::Out(PageOutput::TogglePage(PageSelector::Documents))
            }
            DashboardOutput::NavigateToDocumentsWithStatus(status) => {
                PageMessage::NavigateToDocumentsWithStatus(status)
            }
            DashboardOutput::NavigateToDocumentsWithType(type_) => {
                PageMessage::NavigateToDocumentsWithType(type_)
            }
            DashboardOutput::NavigateToSettings => {
                PageMessage::Out(PageOutput::TogglePage(PageSelector::Preferences))
            }
            DashboardOutput::NavigateToSources => {
                PageMessage::Out(PageOutput::TogglePage(PageSelector::Preferences))
            }
            DashboardOutput::NavigateToOnlineLibrary => {
                PageMessage::Out(PageOutput::TogglePage(PageSelector::OnlineLibrary))
            }
            DashboardOutput::OpenDocument(document) => PageMessage::OpenDocument(document),
            DashboardOutput::Scan => PageMessage::Out(PageOutput::Scan),
        },
        msg => PageMessage::Dashboard(msg),
    }
}

fn map_online_library_message(msg: OnlineLibraryMessage) -> PageMessage {
    match msg {
        OnlineLibraryMessage::Out(output) => match output {
            OnlineLibraryOutput::BookImported => PageMessage::Out(PageOutput::Scan),
        },
        msg => PageMessage::OnlineLibrary(msg),
    }
}

fn map_preferences_message(msg: PreferencesMessage) -> PageMessage {
    match msg {
        PreferencesMessage::Out(output) => match output {
            PreferencesOutput::SourceAdded(url, user_id, passphrase) => {
                PageMessage::AddRemote(url, user_id, passphrase)
            }
            PreferencesOutput::SourceEdited(old_url, new_url, user_id, passphrase) => {
                PageMessage::EditRemote(old_url, new_url, user_id, passphrase)
            }
            PreferencesOutput::SourceDeleted(url) => PageMessage::DeleteRemote(url),
        },
        msg => PageMessage::Preferences(msg),
    }
}

fn map_document_list_message(msg: DocumentListMessage) -> PageMessage {
    match msg {
        DocumentListMessage::Out(message) => match message {
            DocumentListOutput::OpenDetails(document) => PageMessage::OpenDocumentDetails(document),
            DocumentListOutput::OpenDocument(document) => PageMessage::OpenDocument(document),
            DocumentListOutput::NavigateToSettings => {
                PageMessage::Out(PageOutput::TogglePage(PageSelector::Preferences))
            }
            DocumentListOutput::Scan => PageMessage::Out(PageOutput::Scan),
        },
        msg => PageMessage::Documents(msg),
    }
}

fn map_document_details_message(
    fingerprint: Fingerprint,
    msg: DocumentDetailsMessage,
) -> PageMessage {
    match msg {
        DocumentDetailsMessage::Out(message) => match message {
            DocumentDetailsOutput::Close(fingerprint) => {
                PageMessage::CloseDocumentDetails(fingerprint)
            }
            DocumentDetailsOutput::RefreshDocument(document) => {
                PageMessage::Documents(DocumentListMessage::RefreshDocument(document))
            }
            DocumentDetailsOutput::OpenDocument(document) => PageMessage::OpenDocument(document),
            DocumentDetailsOutput::OpenImageViewer(image) => PageMessage::OpenImageViewer(image),
        },
        msg => PageMessage::DocumentDetails(fingerprint, msg),
    }
}

fn map_mu_pdf_viewer_message(fingerprint: Fingerprint, msg: MuPdfViewerMessage) -> PageMessage {
    match msg {
        MuPdfViewerMessage::Out(message) => match message {
            MuPdfViewerOutput::Close(fingerprint, page_info) => {
                PageMessage::CloseMuPdfViewer(fingerprint, page_info)
            }
            MuPdfViewerOutput::OpenDocumentDetails(document) => {
                PageMessage::OpenDocumentDetails(document)
            }
        },
        msg => PageMessage::MuPdfViewer(fingerprint, msg),
    }
}

fn map_epub_viewer_message(fingerprint: Fingerprint, msg: EpubViewerMessage) -> PageMessage {
    match msg {
        EpubViewerMessage::Out(message) => match message {
            EpubViewerOutput::Close(fingerprint, progress_info) => {
                PageMessage::CloseEpubViewer(fingerprint, progress_info)
            }
            EpubViewerOutput::OpenImageViewer(image) => PageMessage::OpenImageViewer(image),
            EpubViewerOutput::Activate => PageMessage::Out(PageOutput::TogglePage(
                PageSelector::EpubViewer(fingerprint),
            )),
            EpubViewerOutput::OpenDocumentDetails(document) => {
                PageMessage::OpenDocumentDetails(document)
            }
        },
        msg => PageMessage::EpubViewer(fingerprint, msg),
    }
}

fn map_image_viewer_message(id: u64, msg: ImageViewerMessage) -> PageMessage {
    match msg {
        ImageViewerMessage::Out(message) => match message {
            ImageViewerOutput::Close(id) => PageMessage::CloseImageViewer(id),
        },
        msg => PageMessage::ImageViewer(id, msg),
    }
}

/// Generate an ISO 8601 UTC timestamp string from the current system time.
fn iso8601_now() -> String {
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;

    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let days = (secs / 86400) as i64;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

pub fn settings_invalidation_subscription<M, F>(
    application_module: Arc<ApplicationModule>,
    f: F,
) -> Subscription<M>
where
    M: Send + 'static,
    F: Fn() -> M + Send + 'static,
    F: Send + Sync + 'static,
{
    let receiver = application_module.subscribe();

    Subscription::run_with(SubscriberState::new(receiver, f), SubscriberState::run)
}
