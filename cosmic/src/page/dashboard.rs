// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;
use std::sync::Arc;

use cosmic::Action;
use cosmic::Apply;
use cosmic::Element;
use cosmic::Task;
use cosmic::cosmic_theme;
use cosmic::iced::Background;
use cosmic::iced::ContentFit;
use cosmic::iced::Length;
use cosmic::iced::alignment::Horizontal;
use cosmic::iced::alignment::Vertical;
use cosmic::task;
use cosmic::theme;
use cosmic::widget;
use cosmic::widget::Column;
use cosmic::widget::Row;
use read_flow_core::api::ReadingStatus;
use read_flow_core::scan::DocumentType;

use crate::ICON_SIZE;
use crate::aggregator::Document;
use crate::app::ContextView;
use crate::client::ClientSelector;
use crate::component::source_picker::source_picker_dialog;
use crate::document_provider::DocumentProvider;
use crate::fl;
use crate::layout::layout;
use crate::page::Page;

#[derive(Debug, Clone)]
pub enum DashboardOutput {
    NavigateToDocuments,
    NavigateToDocumentsWithStatus(ReadingStatus),
    NavigateToDocumentsWithType(DocumentType),
    NavigateToSettings,
    NavigateToSources,
    NavigateToOnlineLibrary,
    OpenDocument(Document),
    Scan,
}

#[derive(Debug, Clone)]
pub enum DashboardMessage {
    LoadDashboard,
    Loaded(DashboardData),
    LoadingFailed(String),
    CoversLoaded(HashMap<String, Vec<u8>>),
    ReadingStatesLoaded(Vec<(String, f64, String)>),
    OpenDocument(Document),
    PickDocumentSource(String),
    CancelFormatPick,
    NavigateToDocuments,
    NavigateToDocumentsWithStatus(ReadingStatus),
    NavigateToDocumentsWithType(DocumentType),
    NavigateToSettings,
    NavigateToSources,
    NavigateToOnlineLibrary,
    Scan,
    Out(DashboardOutput),
}

#[derive(Debug, Clone)]
pub struct DashboardData {
    pub documents: Vec<Document>,
    pub sources: Vec<ClientSelector>,
}

#[derive(Debug, Clone)]
struct TypeStat {
    type_: DocumentType,
    count: usize,
}

#[derive(Debug, Clone)]
struct ContinueReadingEntry {
    document: Document,
    fingerprint: String,
    percentage: f64,
    last_updated: String,
}

pub struct DashboardPage {
    document_provider: Arc<DocumentProvider>,
    state: DashboardState,
    covers: HashMap<String, widget::image::Handle>,
    pending_format_pick: Option<Document>,
}

#[derive(Debug, Clone)]
enum DashboardState {
    Loading,
    Empty,
    Populated {
        total_documents: usize,
        reading_count: usize,
        read_count: usize,
        unread_count: usize,
        type_stats: Vec<TypeStat>,
        sources: Vec<ClientSelector>,
        continue_reading: Vec<ContinueReadingEntry>,
    },
}

fn card_button_class() -> cosmic::theme::Button {
    cosmic::theme::Button::Custom {
        active: Box::new(|_, theme| {
            let cosmic = theme.cosmic();
            cosmic::widget::button::Style {
                background: Some(Background::Color(cosmic.primary.base.into())),
                border_radius: cosmic.corner_radii.radius_s.into(),
                border_width: 1.0,
                border_color: cosmic.accent.base.into(),
                text_color: Some(cosmic.primary.on.into()),
                ..Default::default()
            }
        }),
        disabled: Box::new(|theme| {
            let cosmic = theme.cosmic();
            cosmic::widget::button::Style {
                background: Some(Background::Color(cosmic.primary.base.into())),
                border_radius: cosmic.corner_radii.radius_s.into(),
                border_width: 1.0,
                border_color: cosmic.accent.base.into(),
                text_color: Some(cosmic.primary.on.into()),
                ..Default::default()
            }
        }),
        hovered: Box::new(|_, theme| {
            let cosmic = theme.cosmic();
            cosmic::widget::button::Style {
                background: Some(Background::Color(cosmic.primary.component.hover.into())),
                border_radius: cosmic.corner_radii.radius_s.into(),
                border_width: 1.0,
                border_color: cosmic.accent.base.into(),
                text_color: Some(cosmic.primary.on.into()),
                ..Default::default()
            }
        }),
        pressed: Box::new(|_, theme| {
            let cosmic = theme.cosmic();
            cosmic::widget::button::Style {
                background: Some(Background::Color(cosmic.primary.component.pressed.into())),
                border_radius: cosmic.corner_radii.radius_s.into(),
                border_width: 1.0,
                border_color: cosmic.accent.base.into(),
                text_color: Some(cosmic.primary.on.into()),
                ..Default::default()
            }
        }),
    }
}

impl DashboardPage {
    pub fn new(document_provider: Arc<DocumentProvider>) -> (Self, Task<Action<DashboardMessage>>) {
        (
            Self {
                document_provider,
                state: DashboardState::Loading,
                covers: HashMap::new(),
                pending_format_pick: None,
            },
            task::message(DashboardMessage::LoadDashboard),
        )
    }

    fn compute_stats(documents: &[Document], sources: Vec<ClientSelector>) -> DashboardState {
        if documents.is_empty() {
            return DashboardState::Empty;
        }

        let total_documents = documents.len();
        let mut reading_count = 0usize;
        let mut read_count = 0usize;
        let mut unread_count = 0usize;
        let mut type_counts: HashMap<DocumentType, usize> = HashMap::new();
        let mut continue_reading = Vec::new();

        for doc in documents {
            // Collect all in-progress content fingerprints.
            let mut status = ReadingStatus::Unread;
            let mut reading_fps: Vec<String> = Vec::new();
            for content in &doc.contents {
                *type_counts.entry(content.type_).or_default() += 1;
                match content.status {
                    ReadingStatus::Reading => {
                        status = ReadingStatus::Reading;
                        reading_fps.push(content.fingerprint.clone());
                    }
                    ReadingStatus::Read if status != ReadingStatus::Reading => {
                        status = ReadingStatus::Read;
                    }
                    _ => {}
                }
            }

            match status {
                ReadingStatus::Unread => unread_count += 1,
                ReadingStatus::Reading => {
                    reading_count += 1;
                    let fingerprint = reading_fps.first().cloned().unwrap_or_default();
                    // Reorder contents so the first in-progress file is first,
                    // since OpenDocument uses the first content to pick the viewer.
                    let mut open_doc = doc.clone();
                    if let Some(idx) = open_doc
                        .contents
                        .iter()
                        .position(|c| c.fingerprint == fingerprint)
                    {
                        open_doc.contents.swap(0, idx);
                    }
                    continue_reading.push(ContinueReadingEntry {
                        document: open_doc,
                        fingerprint,
                        percentage: 0.0,
                        last_updated: String::new(),
                    });
                }
                ReadingStatus::Read => read_count += 1,
            }
        }

        let mut type_stats: Vec<TypeStat> = type_counts
            .into_iter()
            .map(|(type_, count)| TypeStat { type_, count })
            .collect();
        type_stats.sort_by(|a, b| b.count.cmp(&a.count));

        DashboardState::Populated {
            total_documents,
            reading_count,
            read_count,
            unread_count,
            type_stats,
            sources,
            continue_reading,
        }
    }

    fn view_loading<'a>(&self) -> Element<'a, DashboardMessage> {
        let cosmic_theme::Spacing { space_s, .. } = theme::active().cosmic().spacing;
        widget::container(
            Row::new()
                .spacing(space_s)
                .align_y(Vertical::Center)
                .push(
                    widget::icon::from_name("content-loading-symbolic")
                        .size(ICON_SIZE)
                        .icon(),
                )
                .push(widget::text(fl!("document-list-loading"))),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
    }

    fn view_empty<'a>(&self) -> Element<'a, DashboardMessage> {
        let cosmic_theme::Spacing {
            space_l, space_xl, ..
        } = theme::active().cosmic().spacing;

        let onboarding_step = |num: &str, title: String, desc: String, action: DashboardMessage| {
            widget::settings::item_row(vec![
                widget::container(
                    widget::text::title4(num.to_string())
                        .apply(widget::container)
                        .width(Length::Fixed(32.0))
                        .align_x(Horizontal::Center),
                )
                .align_y(Vertical::Center)
                .into(),
                Column::new()
                    .spacing(4)
                    .push(widget::text::heading(title))
                    .push(widget::text(desc).size(13))
                    .width(Length::Fill)
                    .into(),
                widget::button::standard(fl!("dashboard-action-go"))
                    .on_press(action)
                    .into(),
            ])
        };

        let steps = widget::settings::section()
            .add(onboarding_step(
                "1",
                fl!("dashboard-onboarding-step-scan-title"),
                fl!("dashboard-onboarding-step-scan-description"),
                DashboardMessage::NavigateToSettings,
            ))
            .add(onboarding_step(
                "2",
                fl!("dashboard-onboarding-step-run-title"),
                fl!("dashboard-onboarding-step-run-description"),
                DashboardMessage::Scan,
            ))
            .add(onboarding_step(
                "3",
                fl!("dashboard-onboarding-step-online-title"),
                fl!("dashboard-onboarding-step-online-description"),
                DashboardMessage::NavigateToOnlineLibrary,
            ))
            .add(onboarding_step(
                "4",
                fl!("dashboard-onboarding-step-remote-title"),
                fl!("dashboard-onboarding-step-remote-description"),
                DashboardMessage::NavigateToSources,
            ));

        widget::container(
            Column::new()
                .spacing(space_l)
                .align_x(Horizontal::Center)
                .max_width(560.0)
                .push(
                    widget::icon::from_svg_bytes(crate::app::APP_ICON)
                        .icon()
                        .size(128),
                )
                .push(widget::text::title2(fl!("dashboard-welcome-title")))
                .push(
                    widget::text(fl!("dashboard-welcome-description"))
                        .width(Length::Fixed(480.0))
                        .center(),
                )
                .push(steps),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .padding(space_xl)
        .into()
    }

    fn view_populated<'a>(
        &'a self,
        total_documents: usize,
        reading_count: usize,
        read_count: usize,
        _unread_count: usize,
        type_stats: &'a [TypeStat],
        sources: &'a [ClientSelector],
        continue_reading: &'a [ContinueReadingEntry],
    ) -> Element<'a, DashboardMessage> {
        let cosmic_theme::Spacing {
            space_s,
            space_m,
            space_l,
            space_xl,
            ..
        } = theme::active().cosmic().spacing;

        let mut content = Column::new().spacing(space_xl).width(Length::Fill);

        // --- Continue Reading section ---
        if !continue_reading.is_empty() {
            let cards: Vec<Element<'_, DashboardMessage>> = continue_reading
                .iter()
                .take(4)
                .map(|entry| self.view_continue_reading_card(entry))
                .collect();

            let cards_row = Row::with_children(cards)
                .spacing(space_m)
                .width(Length::Fill);

            let section = Column::new()
                .spacing(space_s)
                .push(widget::text::title4(fl!("dashboard-continue-reading")))
                .push(cards_row);

            content = content.push(section);
        } else {
            let empty_hint = Row::new()
                .spacing(space_s)
                .align_y(Vertical::Center)
                .push(widget::text::title4(fl!("dashboard-continue-reading")))
                .push(widget::text(fl!("dashboard-continue-reading-empty-hint")).size(13));

            content = content.push(empty_hint);
        }

        // --- Library Overview stats ---
        let stat_card = |count: usize,
                         label: String,
                         status: Option<ReadingStatus>|
         -> Element<'_, DashboardMessage> {
            let msg = status
                .map(DashboardMessage::NavigateToDocumentsWithStatus)
                .unwrap_or(DashboardMessage::NavigateToDocuments);

            widget::button::custom(
                Column::new()
                    .spacing(space_s)
                    .align_x(Horizontal::Center)
                    .push(widget::text::title1(count.to_string()))
                    .push(widget::text(label).size(13))
                    .apply(widget::container)
                    .padding(space_l)
                    .width(Length::Fill)
                    .center_x(Length::Fill),
            )
            .width(Length::FillPortion(1))
            .on_press(msg)
            .class(card_button_class())
            .into()
        };

        let stats_row = Row::new()
            .spacing(space_m)
            .push(stat_card(
                total_documents,
                fl!("dashboard-stat-documents"),
                None,
            ))
            .push(stat_card(
                reading_count,
                fl!("dashboard-stat-reading"),
                Some(ReadingStatus::Reading),
            ))
            .push(stat_card(
                read_count,
                fl!("dashboard-stat-completed"),
                Some(ReadingStatus::Read),
            ));

        let stats_section = Column::new()
            .spacing(space_s)
            .push(widget::text::title4(fl!("dashboard-library-overview")))
            .push(stats_row);

        content = content.push(stats_section);

        // --- Format Breakdown + Sources side by side ---
        let mut details_row = Row::new().spacing(space_l);

        // Format breakdown
        if !type_stats.is_empty() {
            let max_count = type_stats.first().map(|s| s.count).unwrap_or(1).max(1);

            let format_items = type_stats.iter().take(6).fold(
                widget::settings::section().title(fl!("dashboard-format-breakdown")),
                |section, stat| {
                    let bar_width = (stat.count as f32 / max_count as f32) * 100.0;
                    let type_ = stat.type_;
                    let row = widget::settings::item_row(vec![
                        widget::icon::from_name(stat.type_.get_file_type_icon())
                            .size(ICON_SIZE)
                            .icon()
                            .into(),
                        widget::text(stat.type_.label().to_uppercase())
                            .size(12)
                            .width(Length::Fixed(80.0))
                            .into(),
                        widget::container(
                            widget::container(
                                widget::Space::new()
                                    .width(Length::Fixed(bar_width))
                                    .height(8),
                            )
                            .class(cosmic::style::Container::Primary),
                        )
                        .width(Length::Fixed(100.0))
                        .into(),
                        widget::text(stat.count.to_string())
                            .size(13)
                            .width(Length::Fixed(40.0))
                            .into(),
                    ])
                    .align_y(Vertical::Center);
                    section.add(
                        widget::button::custom(row)
                            .class(cosmic::theme::Button::Text)
                            .on_press(DashboardMessage::NavigateToDocumentsWithType(type_)),
                    )
                },
            );
            details_row =
                details_row.push(widget::container(format_items).width(Length::FillPortion(1)));
        }

        // Sources
        if !sources.is_empty() {
            let sources_section = sources.iter().fold(
                widget::settings::section().title(fl!("dashboard-sources")),
                |section, source| {
                    let icon_name = if source.is_local() {
                        "computer-symbolic"
                    } else {
                        "network-server-symbolic"
                    };
                    section.add(
                        widget::settings::item_row(vec![
                            widget::icon::from_name(icon_name)
                                .size(ICON_SIZE)
                                .icon()
                                .into(),
                            widget::text(source.to_string()).width(Length::Fill).into(),
                        ])
                        .align_y(Vertical::Center),
                    )
                },
            );
            details_row =
                details_row.push(widget::container(sources_section).width(Length::FillPortion(1)));
        }

        content = content.push(details_row);

        // --- Quick Actions ---
        let actions = Column::new()
            .spacing(space_s)
            .push(widget::text::title4(fl!("dashboard-quick-actions")))
            .push(
                Row::new()
                    .spacing(space_m)
                    .push(
                        widget::button::suggested(fl!("document-list-run-scan"))
                            .on_press(DashboardMessage::Scan),
                    )
                    .push(
                        widget::button::standard(fl!("online-library-page-title"))
                            .on_press(DashboardMessage::NavigateToOnlineLibrary),
                    )
                    .push(
                        widget::button::standard(fl!("dashboard-all-documents"))
                            .on_press(DashboardMessage::NavigateToDocuments),
                    ),
            );

        content = content.push(actions);

        content
            .apply(layout)
            .apply(widget::scrollable::vertical)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn view_continue_reading_card<'a>(
        &'a self,
        entry: &'a ContinueReadingEntry,
    ) -> Element<'a, DashboardMessage> {
        let cosmic_theme::Spacing { space_xs, .. } = theme::active().cosmic().spacing;

        let title = entry
            .document
            .user_meta
            .title
            .as_deref()
            .or_else(|| {
                entry
                    .document
                    .contents
                    .first()
                    .and_then(|c| c.sources.first())
                    .map(|s| s.path.rsplit('/').next().unwrap_or(&s.path))
            })
            .unwrap_or("Untitled");

        let authors = entry
            .document
            .user_meta
            .authors
            .as_ref()
            .map(|a| a.join(", "));

        let cover_key = entry
            .document
            .user_meta
            .selected_cover_fingerprint
            .as_ref()
            .unwrap_or(&entry.fingerprint);

        let cover_element: Element<'_, DashboardMessage> =
            if let Some(handle) = self.covers.get(cover_key) {
                widget::container(
                    widget::image(handle.clone())
                        .content_fit(ContentFit::Cover)
                        .width(Length::Fill)
                        .height(Length::Fixed(180.0)),
                )
                .clip(true)
                .width(Length::Fill)
                .height(Length::Fixed(180.0))
                .class(cosmic::style::Container::Card)
                .into()
            } else {
                let type_icon = entry
                    .document
                    .contents
                    .first()
                    .map(|c| c.type_.get_file_type_icon())
                    .unwrap_or("text-x-generic-symbolic");

                widget::container(widget::icon::from_name(type_icon).size(48).icon())
                    .clip(true)
                    .width(Length::Fill)
                    .height(Length::Fixed(180.0))
                    .center_x(Length::Fill)
                    .center_y(Length::Fixed(180.0))
                    .class(cosmic::style::Container::Card)
                    .into()
            };

        let pct = (entry.percentage * 100.0) as u32;
        let filled = ((entry.percentage * 100.0).round() as u16).clamp(1, 99);
        let empty = 100u16.saturating_sub(filled);
        let progress_bar = Row::new()
            .push(
                widget::container(widget::Space::new().height(Length::Fixed(4.0)))
                    .class(cosmic::style::Container::Primary)
                    .width(Length::FillPortion(filled)),
            )
            .push(
                widget::Space::new()
                    .width(Length::FillPortion(empty))
                    .height(Length::Fixed(4.0)),
            )
            .width(Length::Fill);

        let mut card_content = Column::new()
            .spacing(space_xs)
            .width(Length::FillPortion(1))
            .push(cover_element)
            .push(
                widget::text(title)
                    .size(13)
                    .wrapping(cosmic::iced::widget::text::Wrapping::WordOrGlyph),
            );

        if let Some(a) = authors {
            card_content = card_content.push(widget::text(a).size(11));
        }

        card_content = card_content
            .push(progress_bar)
            .push(widget::text(format!("{pct}%")).size(11));

        let doc = entry.document.clone();
        widget::button::custom(card_content)
            .on_press(DashboardMessage::OpenDocument(doc))
            .class(card_button_class())
            .width(Length::FillPortion(1))
            .into()
    }
}

impl Page for DashboardPage {
    type Message = DashboardMessage;

    fn view(&self) -> Element<'_, Self::Message> {
        match &self.state {
            DashboardState::Loading => self.view_loading(),
            DashboardState::Empty => self.view_empty(),
            DashboardState::Populated {
                total_documents,
                reading_count,
                read_count,
                unread_count,
                type_stats,
                sources,
                continue_reading,
            } => self.view_populated(
                *total_documents,
                *reading_count,
                *read_count,
                *unread_count,
                type_stats,
                sources,
                continue_reading,
            ),
        }
    }

    fn update(&mut self, message: Self::Message) -> Task<Action<Self::Message>> {
        match message {
            DashboardMessage::LoadDashboard => {
                self.state = DashboardState::Loading;
                let document_provider = self.document_provider.clone();
                task::future(async move {
                    match document_provider.get_documents().await {
                        Ok(documents) => {
                            let sources = document_provider.get_client_selectors().await;
                            DashboardMessage::Loaded(DashboardData {
                                documents: documents.into_iter().collect(),
                                sources,
                            })
                        }
                        Err(error) => DashboardMessage::LoadingFailed(format!("{error}")),
                    }
                })
            }
            DashboardMessage::Loaded(data) => {
                self.state = Self::compute_stats(&data.documents, data.sources);

                let mut tasks: Vec<Task<Action<DashboardMessage>>> = Vec::new();

                // Load covers for continue-reading documents
                if let DashboardState::Populated {
                    continue_reading, ..
                } = &self.state
                {
                    let fingerprints: Vec<String> = continue_reading
                        .iter()
                        .flat_map(|e| {
                            let mut fps = vec![e.fingerprint.clone()];
                            if let Some(ref fp) = e.document.user_meta.selected_cover_fingerprint {
                                fps.push(fp.clone());
                            }
                            fps
                        })
                        .collect();

                    if !fingerprints.is_empty() {
                        let dp = self.document_provider.clone();
                        tasks.push(task::future(async move {
                            let covers = dp.load_covers(fingerprints).await;
                            DashboardMessage::CoversLoaded(covers)
                        }));
                    }

                    // Load reading states for percentage/last_updated
                    let reading_fingerprints: Vec<String> = continue_reading
                        .iter()
                        .map(|e| e.fingerprint.clone())
                        .collect();
                    if !reading_fingerprints.is_empty() {
                        let dp = self.document_provider.clone();
                        tasks.push(task::future(async move {
                            let aggregator = dp.aggregator.read().await;
                            let mut results = Vec::new();
                            for fp in reading_fingerprints {
                                if let Ok(Some(state)) = aggregator.get_reading_state(&fp).await {
                                    results.push((fp, state.percentage, state.last_updated));
                                }
                            }
                            DashboardMessage::ReadingStatesLoaded(results)
                        }));
                    }
                }

                Task::batch(tasks)
            }
            DashboardMessage::LoadingFailed(_error) => {
                self.state = DashboardState::Empty;
                Task::none()
            }
            DashboardMessage::CoversLoaded(cover_bytes) => {
                for (fp, bytes) in cover_bytes {
                    self.covers
                        .insert(fp, widget::image::Handle::from_bytes(bytes));
                }
                Task::none()
            }
            DashboardMessage::ReadingStatesLoaded(states) => {
                if let DashboardState::Populated {
                    continue_reading, ..
                } = &mut self.state
                {
                    for (fp, percentage, last_updated) in states {
                        if let Some(entry) =
                            continue_reading.iter_mut().find(|e| e.fingerprint == fp)
                        {
                            entry.percentage = percentage;
                            entry.last_updated = last_updated;
                        }
                    }
                    // Sort by most recently read
                    continue_reading.sort_by(|a, b| b.last_updated.cmp(&a.last_updated));
                }
                Task::none()
            }
            DashboardMessage::OpenDocument(document) => {
                // If multiple contents are in progress, ask the user to pick.
                let reading_contents: Vec<_> = document
                    .contents
                    .iter()
                    .filter(|c| c.status == ReadingStatus::Reading)
                    .collect();
                if reading_contents.len() > 1 {
                    self.pending_format_pick = Some(document);
                    Task::none()
                } else {
                    task::message(DashboardMessage::Out(DashboardOutput::OpenDocument(
                        document,
                    )))
                }
            }
            DashboardMessage::PickDocumentSource(guid) => {
                if let Some(doc) = self.pending_format_pick.take() {
                    if let Some(single) = doc.with_source_guid(&guid) {
                        return task::message(DashboardMessage::Out(
                            DashboardOutput::OpenDocument(single),
                        ));
                    }
                }
                Task::none()
            }
            DashboardMessage::CancelFormatPick => {
                self.pending_format_pick = None;
                Task::none()
            }
            DashboardMessage::NavigateToDocuments => {
                task::message(DashboardMessage::Out(DashboardOutput::NavigateToDocuments))
            }
            DashboardMessage::NavigateToDocumentsWithStatus(status) => task::message(
                DashboardMessage::Out(DashboardOutput::NavigateToDocumentsWithStatus(status)),
            ),
            DashboardMessage::NavigateToDocumentsWithType(type_) => task::message(
                DashboardMessage::Out(DashboardOutput::NavigateToDocumentsWithType(type_)),
            ),
            DashboardMessage::NavigateToSettings => {
                task::message(DashboardMessage::Out(DashboardOutput::NavigateToSettings))
            }
            DashboardMessage::NavigateToSources => {
                task::message(DashboardMessage::Out(DashboardOutput::NavigateToSources))
            }
            DashboardMessage::NavigateToOnlineLibrary => task::message(DashboardMessage::Out(
                DashboardOutput::NavigateToOnlineLibrary,
            )),
            DashboardMessage::Scan => task::message(DashboardMessage::Out(DashboardOutput::Scan)),
            DashboardMessage::Out(_) => {
                panic!("DashboardMessage::Out should be handled by parent")
            }
        }
    }

    fn dialog(&self) -> Option<Element<'_, Self::Message>> {
        let document = self.pending_format_pick.as_ref()?;

        let title = document.user_meta.title.clone().unwrap_or_else(|| {
            document
                .local_or_any_source()
                .and_then(|(_, s)| std::path::Path::new(&s.path).file_stem()?.to_str())
                .unwrap_or("")
                .to_owned()
        });

        // Show only in-progress contents.
        let mut sources: Vec<_> = document
            .contents
            .iter()
            .filter(|c| c.status == ReadingStatus::Reading)
            .flat_map(|c| c.sources.iter().map(move |s| (c, s)))
            .collect();
        sources.sort_by(|(ac, as_), (bc, bs)| {
            ac.type_
                .as_str()
                .cmp(bc.type_.as_str())
                .then_with(|| as_.client.is_local().cmp(&bs.client.is_local()))
        });

        let doc_covers: HashMap<String, widget::image::Handle> = document
            .contents
            .iter()
            .filter_map(|c| {
                self.covers
                    .get(&c.fingerprint)
                    .map(|h| (c.fingerprint.clone(), h.clone()))
            })
            .collect();

        Some(source_picker_dialog(
            fl!("document-list-pick-source-title"),
            Some(title),
            sources,
            doc_covers,
            DashboardMessage::PickDocumentSource,
            DashboardMessage::CancelFormatPick,
        ))
    }

    fn view_context(&self) -> ContextView<'_, Self::Message> {
        ContextView {
            title: fl!("dashboard-page-title"),
            content: widget::text(fl!("dashboard-page-title")).into(),
        }
    }
}
