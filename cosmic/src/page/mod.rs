// SPDX-License-Identifier: GPL-3.0-or-later
// pages
mod document_details;
mod document_list;
mod epub_viewer;
mod pdf_viewer;
mod settings;
mod sources;
mod traits;

use core::panic;
use std::any::Any;
use std::sync::Arc;

use archive_organizer::client::FilesClient;
use archive_organizer::db::dao::RemoteDao;
use cosmic::Action;
use cosmic::Apply;
use cosmic::Element;
use cosmic::Task;
use cosmic::iced::Length;
use cosmic::iced::Subscription;
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::alignment::Vertical;
use cosmic::iced::core::SmolStr;
use cosmic::iced::keyboard::Key;
use cosmic::iced::keyboard::Modifiers;
use cosmic::task;
use cosmic::widget;
use indexmap::IndexMap;
use provider::sync::Invalidated;
use tokio::sync::broadcast;
pub use traits::Page;
use url::Url;

use crate::ApplicationModule;
use crate::aggregator::Aggregator;
use crate::aggregator::Document;
use crate::aggregator::DocumentType;
use crate::app::ContextView;
use crate::client::ClientSelector;
use crate::cosmic_ext::ActionExt;
use crate::document_provider::DocumentProvider;
use crate::fl;
use crate::page::document_details::DocumentDetails;
use crate::page::document_details::DocumentDetailsMessage;
use crate::page::document_details::DocumentDetailsOutput;
use crate::page::document_list::DocumentList;
use crate::page::document_list::DocumentListMessage;
use crate::page::document_list::DocumentListOutput;
use crate::page::epub_viewer::EpubViewer;
use crate::page::epub_viewer::EpubViewerMessage;
use crate::page::epub_viewer::EpubViewerOutput;
use crate::page::pdf_viewer::PdfViewer;
use crate::page::pdf_viewer::PdfViewerMessage;
use crate::page::pdf_viewer::PdfViewerOutput;
use crate::page::settings::SettingsMessage;
use crate::page::settings::SettingsPage;
use crate::page::sources::SourcesMessage;
use crate::page::sources::SourcesOutput;
use crate::page::sources::SourcesPage;

type Fingerprint = String;

pub struct Pages {
    pub(crate) document_provider: Arc<DocumentProvider>,

    sources: SourcesPage,
    documents: DocumentList,
    document_details: IndexMap<Fingerprint, DocumentDetails>,
    epub_viewers: IndexMap<Fingerprint, EpubViewer>,
    pdf_viewers: IndexMap<Fingerprint, PdfViewer>,
    settings: SettingsPage,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PageSelector {
    Sources,
    Documents,
    DocumentDetails(Fingerprint),
    EpubViewer(Fingerprint),
    PdfViewer(Fingerprint),
    Settings,
}

#[derive(Debug, Clone)]
pub enum PageOutput {
    PageAdded(PageSelector, &'static str),
    TogglePage(PageSelector),
    PageRemoved(PageSelector),
}

#[derive(Debug, Clone)]
pub enum PageMessage {
    Sources(SourcesMessage),
    AddRemote(Url, String, String), // url, user_id, passphrase
    DeleteRemote(Url),
    Documents(DocumentListMessage),
    DocumentDetails(Fingerprint, DocumentDetailsMessage),
    OpenDocumentDetails(Document),
    CloseDocumentDetails(Fingerprint),
    EpubViewer(Fingerprint, EpubViewerMessage),
    OpenDocument(Document),
    CloseEpubViewer(Fingerprint, Option<String>),
    PdfViewer(Fingerprint, PdfViewerMessage),
    ClosePdfViewer(Fingerprint, Option<usize>),
    Settings(SettingsMessage),
    KeyEvent(PageSelector, Modifiers, Key, Option<SmolStr>),
    ModifiersChanged(PageSelector, Modifiers),
    Refresh,
    Noop,
    Out(PageOutput),
}

impl From<SourcesMessage> for PageMessage {
    fn from(source: SourcesMessage) -> Self {
        Self::Sources(source)
    }
}

impl From<DocumentListMessage> for PageMessage {
    fn from(source: DocumentListMessage) -> Self {
        Self::Documents(source)
    }
}

impl From<SettingsMessage> for PageMessage {
    fn from(source: SettingsMessage) -> Self {
        Self::Settings(source)
    }
}

macro_rules! with_active_page {
    ($self:expr, $selector:expr, |$page:ident, $mapper:ident| $body:expr) => {
        match $selector {
            PageSelector::Sources => {
                let $page = Some(&$self.sources);
                let $mapper = map_sources_message;
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
            PageSelector::PdfViewer(fingerprint) => {
                let $page = $self.pdf_viewers.get(fingerprint);
                let fingerprint = fingerprint.clone();
                let $mapper = move |msg| map_pdf_viewer_message(fingerprint.clone(), msg);
                $body
            }
            PageSelector::Settings => {
                let $page = Some(&$self.settings);
                let $mapper = map_settings_message;
                $body
            }
        }
    };
}

fn page_not_found<'a, M: 'a>() -> Element<'a, M> {
    widget::text::title1(fl!("page-not-found"))
        .apply(widget::container)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Horizontal::Center)
        .align_y(Vertical::Center)
        .into()
}

impl Pages {
    pub fn new(application_module: Arc<ApplicationModule>) -> (Self, Task<Action<PageMessage>>) {
        let (sources, init_sources) = SourcesPage::new(application_module.clone());

        // Get remote clients from the application module
        let remote_clients = application_module
            .connection_pool()
            .select_all_remotes()
            .unwrap_or_default()
            .into_iter()
            .map(|remote| {
                let remote_connection: Url = remote.base_url.parse()?;
                let client = FilesClient::new(
                    remote_connection,
                    remote.user_id.clone(),
                    remote.passphrase.clone(),
                )?;
                Ok(client)
            })
            .collect::<anyhow::Result<_>>()
            .unwrap_or_else(|_e| {
                // tracing::error!("Failed to get remote clients: {}", e);
                Vec::new()
            });

        let clients = remote_clients
            .clone()
            .into_iter()
            .map(Into::into)
            .chain(Some(application_module.clone().into()))
            .collect::<Vec<_>>();

        let document_provider = Arc::new(DocumentProvider::new(Aggregator::new(
            clients,
            application_module.clone(),
        )));

        let (settings, init_settings) =
            SettingsPage::new(application_module.clone(), document_provider.clone());

        let (documents, init_documents) = DocumentList::new(document_provider.clone());

        let tasks = vec![
            init_sources.map(ActionExt::map_into),
            init_documents.map(ActionExt::map_into),
            init_settings.map(ActionExt::map_into),
        ];

        (
            Self {
                document_provider,
                sources,
                documents,
                document_details: Default::default(),
                epub_viewers: Default::default(),
                pdf_viewers: Default::default(),
                settings,
            },
            task::batch(tasks),
        )
    }

    pub fn display_name<'a>(&'a self, page_selector: &'a PageSelector) -> String {
        match &page_selector {
            PageSelector::Sources => fl!("app-file-sources"),
            PageSelector::Documents => "Documents".to_string(),
            PageSelector::DocumentDetails(fingerprint) => {
                self.document_details[fingerprint].display_name()
            }
            PageSelector::EpubViewer(fingerprint) => self.epub_viewers[fingerprint].display_name(),
            PageSelector::PdfViewer(fingerprint) => self.pdf_viewers[fingerprint].display_name(),
            PageSelector::Settings => fl!("settings-page-title"),
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
                Task::batch(messages)
            }
            PageMessage::DocumentDetails(fingerprint, message) => self.document_details
                [&fingerprint]
                .update(message)
                .map(move |action| {
                    action.map(|msg| map_document_details_message(fingerprint.clone(), msg))
                }),
            PageMessage::AddRemote(url, user_id, passphrase) => {
                let document_provider = self.document_provider.clone();
                task::future(async move {
                    tracing::debug!("adding remote client: {url}");
                    document_provider
                        .add_client(FilesClient::new(url, user_id, passphrase).unwrap().into())
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
            PageMessage::Sources(sources_message) => self
                .sources
                .update(sources_message)
                .map(move |action| action.map(map_sources_message)),
            PageMessage::Documents(document_list_message) => self
                .documents
                .update(document_list_message)
                .map(move |action| action.map(map_document_list_message)),
            PageMessage::Settings(settings_message) => self
                .settings
                .update(settings_message)
                .map(move |action| action.map(map_settings_message)),
            PageMessage::CloseDocumentDetails(fingerprint) => {
                let _ = self.document_details.swap_remove(&fingerprint);
                task::message(PageMessage::Out(PageOutput::PageRemoved(
                    PageSelector::DocumentDetails(fingerprint),
                )))
            }
            PageMessage::OpenDocumentDetails(document) => {
                let fingerprint = document.metadata.fingerprint.clone();
                let document_icon = document.metadata.type_.get_file_type_icon();

                // Only create new document_details if it does not yet exist
                if self.document_details.contains_key(&fingerprint) {
                    // Page already exists, just navigate to it
                    task::message(PageMessage::Out(PageOutput::TogglePage(
                        PageSelector::DocumentDetails(fingerprint),
                    )))
                } else {
                    let fingerprint_1 = fingerprint.clone();
                    let fingerprint_2 = fingerprint.clone();
                    let (document_details, initialization) =
                        DocumentDetails::new(document, self.documents.document_provider.clone());
                    self.document_details
                        .insert(fingerprint.clone(), document_details);
                    initialization
                        .map(move |action| {
                            let fingerprint = fingerprint_1.clone();
                            action.map(move |msg| map_document_details_message(fingerprint, msg))
                        })
                        .chain(task::message(PageMessage::Out(PageOutput::PageAdded(
                            PageSelector::DocumentDetails(fingerprint_2),
                            document_icon,
                        ))))
                }
            }
            PageMessage::KeyEvent(page, modifiers, key, text) => match page {
                PageSelector::EpubViewer(fingerprint) => self.epub_viewers[&fingerprint]
                    .update(EpubViewerMessage::Key(modifiers, key, text))
                    .map(move |action| {
                        action.map(|msg| map_epub_viewer_message(fingerprint.clone(), msg))
                    }),
                PageSelector::PdfViewer(fingerprint) => self.pdf_viewers[&fingerprint]
                    .update(PdfViewerMessage::Key(modifiers, key, text))
                    .map(move |action| {
                        action.map(|msg| map_pdf_viewer_message(fingerprint.clone(), msg))
                    }),
                _ => Task::none(),
            },
            PageMessage::ModifiersChanged(page, modifiers) => match page {
                PageSelector::EpubViewer(fingerprint) => self.epub_viewers[&fingerprint]
                    .update(EpubViewerMessage::ModifiersChanged(modifiers))
                    .map(move |action| {
                        action.map(|msg| map_epub_viewer_message(fingerprint.clone(), msg))
                    }),
                PageSelector::PdfViewer(fingerprint) => self.pdf_viewers[&fingerprint]
                    .update(PdfViewerMessage::ModifiersChanged(modifiers))
                    .map(move |action| {
                        action.map(|msg| map_pdf_viewer_message(fingerprint.clone(), msg))
                    }),
                _ => Task::none(),
            },
            PageMessage::PdfViewer(fingerprint, message) => self.pdf_viewers[&fingerprint]
                .update(message)
                .map(move |action| {
                    action.map(|msg| map_pdf_viewer_message(fingerprint.clone(), msg))
                }),
            PageMessage::ClosePdfViewer(fingerprint, page) => {
                let _ = self.pdf_viewers.swap_remove(&fingerprint);

                let mut tasks = vec![task::message(PageMessage::Out(PageOutput::PageRemoved(
                    PageSelector::PdfViewer(fingerprint.clone()),
                )))];

                if let Some(page) = page {
                    let document_provider = self.document_provider.clone();
                    let fp = fingerprint;
                    tasks.push(task::future(async move {
                        let now = iso8601_now();
                        let progress = archive_organizer::api::ReadingProgress {
                            fingerprint: fp,
                            progress: format!("{{\"page\":{page}}}"),
                            last_updated: now,
                        };
                        let aggregator = document_provider.aggregator.read().await;
                        if let Err(e) = aggregator.upsert_reading_progress(progress).await {
                            tracing::warn!("failed to save reading progress: {e}");
                        }
                        PageMessage::Noop
                    }));
                }

                Task::batch(tasks)
            }
            PageMessage::EpubViewer(fingerprint, message) => self.epub_viewers[&fingerprint]
                .update(message)
                .map(move |action| {
                    action.map(|msg| map_epub_viewer_message(fingerprint.clone(), msg))
                }),
            PageMessage::OpenDocument(document) => match &document.metadata.type_ {
                DocumentType::Pdf => self.open_pdf_viewer(document),
                DocumentType::Epub => self.open_epub_viewer(document),
                DocumentType::Mobi => {
                    let document_provider = self.document_provider.clone();
                    task::future(async move {
                        if let Err(e) = document_provider.open_document(document).await {
                            tracing::error!("Failed to open file: {e}");
                        }
                        PageMessage::Noop
                    })
                }
            },
            PageMessage::CloseEpubViewer(fingerprint, progress_json) => {
                let _ = self.epub_viewers.swap_remove(&fingerprint);

                let mut tasks = vec![task::message(PageMessage::Out(PageOutput::PageRemoved(
                    PageSelector::EpubViewer(fingerprint.clone()),
                )))];

                if let Some(progress_json) = progress_json {
                    let document_provider = self.document_provider.clone();
                    let fp = fingerprint;
                    tasks.push(task::future(async move {
                        let now = iso8601_now();
                        let progress = archive_organizer::api::ReadingProgress {
                            fingerprint: fp,
                            progress: progress_json,
                            last_updated: now,
                        };
                        let aggregator = document_provider.aggregator.read().await;
                        if let Err(e) = aggregator.upsert_reading_progress(progress).await {
                            tracing::warn!("failed to save reading progress: {e}");
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

    fn open_pdf_viewer(&mut self, document: Document) -> Task<Action<PageMessage>> {
        let fingerprint = document.metadata.fingerprint.clone();

        if self.pdf_viewers.contains_key(&fingerprint) {
            task::message(PageMessage::Out(PageOutput::TogglePage(
                PageSelector::PdfViewer(fingerprint),
            )))
        } else {
            let fingerprint_1 = fingerprint.clone();
            let fingerprint_2 = fingerprint.clone();
            let (pdf_viewer, initialization) =
                PdfViewer::new(document, self.document_provider.clone());
            self.pdf_viewers.insert(fingerprint.clone(), pdf_viewer);
            initialization
                .map(move |action| {
                    let fingerprint = fingerprint_1.clone();
                    action.map(move |msg| map_pdf_viewer_message(fingerprint, msg))
                })
                .chain(task::message(PageMessage::Out(PageOutput::PageAdded(
                    PageSelector::PdfViewer(fingerprint_2),
                    "application-pdf-symbolic",
                ))))
        }
    }

    fn open_epub_viewer(&mut self, document: Document) -> Task<Action<PageMessage>> {
        let fingerprint = document.metadata.fingerprint.clone();

        if self.epub_viewers.contains_key(&fingerprint) {
            task::message(PageMessage::Out(PageOutput::TogglePage(
                PageSelector::EpubViewer(fingerprint),
            )))
        } else {
            let fingerprint_1 = fingerprint.clone();
            let fingerprint_2 = fingerprint.clone();
            let (epub_viewer, initialization) =
                EpubViewer::new(document, self.document_provider.clone());
            self.epub_viewers.insert(fingerprint.clone(), epub_viewer);
            initialization
                .map(move |action| {
                    let fingerprint = fingerprint_1.clone();
                    action.map(move |msg| map_epub_viewer_message(fingerprint, msg))
                })
                .chain(task::message(PageMessage::Out(PageOutput::PageAdded(
                    PageSelector::EpubViewer(fingerprint_2),
                    "application-epub+zip",
                ))))
        }
    }
}

fn map_sources_message(msg: SourcesMessage) -> PageMessage {
    match msg {
        SourcesMessage::Out(message) => match message {
            SourcesOutput::AddedSource(url, user_id, passphrase) => {
                PageMessage::AddRemote(url, user_id, passphrase)
            }
            SourcesOutput::DeletedSource(url) => PageMessage::DeleteRemote(url),
        },
        msg => msg.into(),
    }
}

fn map_document_list_message(msg: DocumentListMessage) -> PageMessage {
    match msg {
        DocumentListMessage::Out(message) => match message {
            DocumentListOutput::OpenDetails(document) => PageMessage::OpenDocumentDetails(document),
            DocumentListOutput::OpenDocument(document) => PageMessage::OpenDocument(document),
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
        },
        msg => PageMessage::DocumentDetails(fingerprint, msg),
    }
}

fn map_pdf_viewer_message(fingerprint: Fingerprint, msg: PdfViewerMessage) -> PageMessage {
    match msg {
        PdfViewerMessage::Out(message) => match message {
            PdfViewerOutput::Close(fingerprint, page) => {
                PageMessage::ClosePdfViewer(fingerprint, page)
            }
        },
        msg => PageMessage::PdfViewer(fingerprint, msg),
    }
}

fn map_epub_viewer_message(fingerprint: Fingerprint, msg: EpubViewerMessage) -> PageMessage {
    match msg {
        EpubViewerMessage::Out(message) => match message {
            EpubViewerOutput::Close(fingerprint, page) => {
                PageMessage::CloseEpubViewer(fingerprint, page)
            }
        },
        msg => PageMessage::EpubViewer(fingerprint, msg),
    }
}

fn map_settings_message(msg: SettingsMessage) -> PageMessage {
    PageMessage::Settings(msg)
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

    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm)
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
{
    use cosmic::iced_futures::futures::SinkExt;
    use cosmic::iced_futures::futures::channel::mpsc;

    let mut receiver = application_module.subscribe();
    Subscription::run_with_id(
        Invalidated.type_id(),
        cosmic::iced::stream::channel(4, move |mut sender: mpsc::Sender<M>| async move {
            loop {
                match receiver.recv().await {
                    Ok(_) => {
                        if sender.send(f()).await.is_err() {
                            // Channel closed, stop the subscription
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        // Sender dropped, stop the subscription
                        break;
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        // Missed some messages, but continue listening
                        // Still send a notification since data has changed
                        if sender.send(f()).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }),
    )
}
