// SPDX-License-Identifier: AGPL-3.0-or-later
//! Activity history page.
//!
//! Renders the structured audit trail ReadFlow keeps for every mutation and
//! observation (scans, tag edits, merges, deletions, maintenance). Events are
//! stored as codes + typed parameters — never localized sentences — and this
//! page localizes them for display: newest first, each row clickable to open
//! the context pane with the ordered child events, actor, channel, and target
//! snapshots.
//!
//! @feature: admin.activity_history
//! @feature: documents.activity_history

use std::sync::Arc;

use cosmic::Action;
use cosmic::Element;
use cosmic::Task;
use cosmic::iced::Alignment;
use cosmic::iced::Color;
use cosmic::iced::Length;
use cosmic::task;
use cosmic::widget;
use read_flow_core::activity::ActivityCursor;
use read_flow_core::activity::ActivityDetail;
use read_flow_core::activity::ActivityEvent;
use read_flow_core::activity::ActivityOperation;
use read_flow_core::activity::ActivityPage as ActivityPageData;
use read_flow_core::activity::ActivityTarget;
use read_flow_core::audit::AuditActor;
use read_flow_core::audit::AuditActorKind;
use read_flow_core::audit::AuditChannel;
use read_flow_core::audit::AuditOutcome;
use read_flow_core::audit::OperationStatus;
use read_flow_core::audit::OperationType;
use serde_json::Value;
use time::Duration;
use time::OffsetDateTime;
use time::UtcOffset;
use time::macros::format_description;

use crate::ApplicationModule;
use crate::app::ContextView;
use crate::client::Client;
use crate::fl;
use crate::page::traits::Page;

/// Operations fetched per page.
const PAGE_SIZE: usize = 50;

const DIM: Color = Color::from_rgb(0.55, 0.55, 0.55);
const CARD_BG: Color = Color::from_rgba(0.5, 0.5, 0.5, 0.08);

#[derive(Debug, Clone)]
pub enum ActivityOutput {
    OpenContext,
    CloseContext,
}

#[derive(Debug, Clone)]
pub enum ActivityMessage {
    /// (re)load the first page.
    Refresh,
    /// load the next page.
    LoadMore,
    PageLoaded(Result<ActivityPageData, String>),
    /// Open an operation's details in the context pane.
    OpSelected(String),
    DetailLoaded(Box<Result<ActivityDetail, String>>),
    RelatedLoaded(Box<Result<Vec<ActivityDetail>, String>>),
    ClearSelection,
    Key(
        cosmic::iced::keyboard::Modifiers,
        cosmic::iced::keyboard::Key,
    ),
    Out(ActivityOutput),
}

/// A list row: the operation overview (without its events).
struct OperationRow {
    op: ActivityOperation,
    summary: String,
}

/// The context-pane selection.
struct Selection {
    op: ActivityOperation,
    detail: Option<ActivityDetail>,
    related: Vec<ActivityDetail>,
    error: Option<String>,
    related_loading: bool,
}

pub struct ActivityPage {
    application_module: Arc<ApplicationModule>,
    operations: Vec<OperationRow>,
    next_cursor: Option<ActivityCursor>,
    loading: bool,
    loaded: bool,
    error: Option<String>,
    selected: Option<Selection>,
}

impl ActivityPage {
    pub fn new(
        application_module: Arc<ApplicationModule>,
    ) -> (Self, Task<Action<ActivityMessage>>) {
        let page = Self {
            application_module,
            operations: Vec::new(),
            next_cursor: None,
            loading: false,
            loaded: false,
            error: None,
            selected: None,
        };
        (page, task::message(ActivityMessage::Refresh))
    }
}

impl ActivityPage {
    /// Fetch the first (or next) page of operations from the local database.
    async fn fetch_page(
        module: Arc<ApplicationModule>,
        cursor: Option<ActivityCursor>,
    ) -> ActivityMessage {
        let client = Client::Local(module);
        let cursor_ref = cursor
            .as_ref()
            .map(|c| (c.started_at.as_str(), c.operation_id.as_str()));
        let page = client
            .get_activity(PAGE_SIZE, cursor_ref)
            .await
            .map_err(|e| e.to_string());
        ActivityMessage::PageLoaded(page)
    }

    /// Fetch one full operation (code + events).
    async fn fetch_detail(module: Arc<ApplicationModule>, operation_id: String) -> ActivityMessage {
        let client = Client::Local(module);
        match client.get_activity_operation(&operation_id).await {
            Ok(Some(detail)) => ActivityMessage::DetailLoaded(Box::new(Ok(detail))),
            Ok(None) => ActivityMessage::DetailLoaded(Box::new(Err(format!(
                "operation {operation_id} not found"
            )))),
            Err(e) => ActivityMessage::DetailLoaded(Box::new(Err(e.to_string()))),
        }
    }

    /// Fetch the full history of every document the operation touched. This is
    /// how a merged-away document's history stays reachable from the winner.
    async fn fetch_related(
        module: Arc<ApplicationModule>,
        operation_id: String,
    ) -> ActivityMessage {
        let client = Client::Local(module);
        let document_ids = {
            let detail = match client.get_activity_operation(&operation_id).await {
                Ok(Some(detail)) => detail,
                _ => return ActivityMessage::RelatedLoaded(Box::new(Ok(Vec::new()))),
            };
            let mut ids: Vec<String> = Vec::new();
            for event in &detail.events {
                for target in &event.targets {
                    if target.target_kind == "document" && !ids.contains(&target.target_id) {
                        ids.push(target.target_id.clone());
                    }
                }
            }
            ids
        };
        let mut result: Vec<ActivityDetail> = Vec::new();
        for id in document_ids {
            match client.get_document_activity(&id).await {
                Ok(details) => {
                    for detail in details {
                        if !result.iter().any(|d| d.operation.id == detail.operation.id) {
                            result.push(detail);
                        }
                    }
                }
                Err(e) => {
                    return ActivityMessage::RelatedLoaded(Box::new(Err(e.to_string())));
                }
            }
        }
        result.sort_by(|a, b| b.operation.started_at.cmp(&a.operation.started_at));
        ActivityMessage::RelatedLoaded(Box::new(Ok(result)))
    }

    fn recompute(&mut self) {
        self.operations
            .sort_by(|a, b| b.op.started_at.cmp(&a.op.started_at));
    }
}

impl Page for ActivityPage {
    type Message = ActivityMessage;

    fn view(&self) -> Element<'_, Self::Message> {
        widget::column::with_children(vec![self.header(), self.operation_list()])
            .spacing(12)
            .padding(16)
            .into()
    }

    fn view_context(&self) -> ContextView<'_, Self::Message> {
        let content = match &self.selected {
            Some(selection) => self.selection_details(selection),
            None => widget::text::body(fl!("activity-select-hint")).into(),
        };
        ContextView {
            title: fl!("activity-details-title"),
            content,
        }
    }

    fn update(&mut self, message: Self::Message) -> Task<Action<Self::Message>> {
        match message {
            ActivityMessage::Refresh => {
                self.operations.clear();
                self.next_cursor = None;
                self.loading = true;
                self.loaded = false;
                self.error = None;
                let module = self.application_module.clone();
                task::future(async move { Self::fetch_page(module, None).await })
            }
            ActivityMessage::LoadMore => {
                if self.loading || self.next_cursor.is_none() {
                    return Task::none();
                }
                self.loading = true;
                let module = self.application_module.clone();
                let cursor = self.next_cursor.clone();
                task::future(async move { Self::fetch_page(module, cursor).await })
            }
            ActivityMessage::PageLoaded(result) => {
                self.loading = false;
                match result {
                    Ok(page) => {
                        let existing: Vec<String> = self
                            .operations
                            .iter()
                            .map(|row| row.op.id.clone())
                            .collect();
                        for op in page.operations {
                            if existing.contains(&op.id) {
                                continue;
                            }
                            let summary = operation_summary(&op);
                            self.operations.push(OperationRow { op, summary });
                        }
                        self.next_cursor = page.next_cursor;
                        self.loaded = true;
                        self.recompute();
                    }
                    Err(message) => self.error = Some(message),
                }
                Task::none()
            }
            ActivityMessage::OpSelected(id) => {
                let Some(row) = self.operations.iter().find(|row| row.op.id == id) else {
                    return Task::none();
                };
                self.selected = Some(Selection {
                    op: row.op.clone(),
                    detail: None,
                    related: Vec::new(),
                    error: None,
                    related_loading: false,
                });
                let module = self.application_module.clone();
                let open = task::message(ActivityMessage::Out(ActivityOutput::OpenContext))
                    .map(cosmic::Action::App);
                let load =
                    task::future(async move { Self::fetch_detail(module, id.clone()).await })
                        .map(cosmic::Action::App);
                task::batch(vec![open, load])
            }
            ActivityMessage::DetailLoaded(result) => {
                let selection = match &mut self.selected {
                    Some(selection) => selection,
                    None => return Task::none(),
                };
                match *result {
                    Ok(detail) => {
                        let has_document_targets = detail.events.iter().any(|event| {
                            event
                                .targets
                                .iter()
                                .any(|target| target.target_kind == "document")
                        });
                        selection.detail = Some(detail);
                        selection.error = None;
                        if has_document_targets && !selection.related_loading {
                            selection.related_loading = true;
                            let module = self.application_module.clone();
                            let id = selection.op.id.clone();
                            return task::future(async move {
                                Self::fetch_related(module, id.clone()).await
                            });
                        }
                    }
                    Err(e) => selection.error = Some(e),
                }
                Task::none()
            }
            ActivityMessage::RelatedLoaded(result) => {
                if let Some(selection) = &mut self.selected {
                    selection.related_loading = false;
                    match *result {
                        Ok(related) => selection.related = related,
                        Err(e) => {
                            if selection.error.is_none() {
                                selection.error = Some(e);
                            }
                        }
                    }
                }
                Task::none()
            }
            ActivityMessage::ClearSelection => {
                self.selected = None;
                task::message(ActivityMessage::Out(ActivityOutput::CloseContext))
            }
            ActivityMessage::Key(_modifiers, key) => {
                if self.selected.is_some()
                    && matches!(
                        key,
                        cosmic::iced::keyboard::Key::Named(
                            cosmic::iced::keyboard::key::Named::Escape
                        )
                    )
                {
                    return task::message(ActivityMessage::ClearSelection);
                }
                Task::none()
            }
            ActivityMessage::Out(_) => Task::none(),
        }
    }
}

impl ActivityPage {
    fn header(&self) -> Element<'_, ActivityMessage> {
        let refresh = {
            let b = widget::button::standard(fl!("activity-refresh"));
            if self.loading {
                b
            } else {
                b.on_press(ActivityMessage::Refresh)
            }
        };
        let load_more = {
            let b = widget::button::standard(fl!("activity-load-more"));
            if self.loading || self.next_cursor.is_none() {
                b
            } else {
                b.on_press(ActivityMessage::LoadMore)
            }
        };
        let (errors, interrupted) = self.operations.iter().fold((0, 0), |(e, i), row| {
            (
                e + usize::from(row.op.status == OperationStatus::Failed),
                i + usize::from(row.op.status == OperationStatus::Interrupted),
            )
        });
        widget::Row::with_children(vec![
            widget::text::title3(fl!("activity-page-title")).into(),
            widget::text::body(fl!(
                "activity-summary",
                errors = errors,
                interrupted = interrupted
            ))
            .class(cosmic::theme::Text::Color(DIM))
            .into(),
            refresh.into(),
            load_more.into(),
        ])
        .spacing(12)
        .align_y(Alignment::Center)
        .into()
    }

    fn operation_list(&self) -> Element<'_, ActivityMessage> {
        if let Some(message) = &self.error {
            return widget::text::body(fl!("activity-error", message = message.as_str()))
                .class(cosmic::theme::Text::Color(Color::from_rgb8(
                    0xC6, 0x2E, 0x2E,
                )))
                .into();
        }
        if self.loading && !self.loaded {
            return widget::text::body(fl!("activity-loading")).into();
        }
        if self.operations.is_empty() {
            return widget::text::body(fl!("activity-empty"))
                .class(cosmic::theme::Text::Color(DIM))
                .into();
        }
        let rows: Vec<Element<'_, ActivityMessage>> = self
            .operations
            .iter()
            .map(|row| self.operation_row(row))
            .collect();
        widget::scrollable(widget::column::with_children(rows).spacing(6)).into()
    }

    fn operation_row(&self, row: &OperationRow) -> Element<'_, ActivityMessage> {
        let selected = self
            .selected
            .as_ref()
            .is_some_and(|selection| selection.op.id == row.op.id);
        let bg = if selected {
            Color::from_rgba(0.40, 0.60, 1.0, 0.14)
        } else {
            CARD_BG
        };
        let op = &row.op;
        let dry_run = if op.dry_run {
            widget::text::body(fl!("activity-dry-run"))
                .size(11)
                .class(cosmic::theme::Text::Color(Color::from_rgb8(
                    0xD9, 0x8E, 0x1F,
                )))
                .into()
        } else {
            widget::text::body("").into()
        };
        let body = widget::column::with_children(vec![
            widget::Row::with_children(vec![
                widget::text::body(operation_type_label(op.operation_type)).into(),
                dry_run,
                widget::text::body(channel_label(op.channel))
                    .size(11)
                    .class(cosmic::theme::Text::Color(DIM))
                    .into(),
            ])
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
            widget::text::body(row.summary.clone())
                .class(cosmic::theme::Text::Color(DIM))
                .into(),
            widget::text::body(format!(
                "{} · {}",
                actor_label(&op.actor),
                friendly_when(op.started_at.as_str())
            ))
            .size(11)
            .class(cosmic::theme::Text::Color(DIM))
            .into(),
        ])
        .spacing(4);

        widget::mouse_area(
            widget::container(body)
                .padding(8)
                .width(Length::Fill)
                .style(move |_theme: &cosmic::Theme| rounded_bg(bg, 6.0)),
        )
        .on_press(ActivityMessage::OpSelected(row.op.id.clone()))
        .into()
    }

    fn selection_details(&self, selection: &Selection) -> Element<'_, ActivityMessage> {
        let op = &selection.op;
        let mut children: Vec<Element<'_, ActivityMessage>> = vec![
            widget::text::heading(operation_type_label(op.operation_type)).into(),
            detail_row(
                fl!("activity-detail-started"),
                friendly_when(&op.started_at),
            ),
        ];
        if let Some(completed) = &op.completed_at {
            children.push(detail_row(
                fl!("activity-detail-completed"),
                friendly_when(completed),
            ));
        }
        children.push(detail_row(
            fl!("activity-detail-actor"),
            actor_label(&op.actor),
        ));
        children.push(detail_row(
            fl!("activity-detail-channel"),
            channel_label(op.channel),
        ));
        children.push(detail_row(
            fl!("activity-detail-status"),
            status_label(op.status),
        ));
        if op.dry_run {
            children.push(detail_row(fl!("activity-dry-run"), String::new()));
        }
        if let Some(code) = &op.error_code {
            children.push(detail_row(fl!("activity-detail-error-code"), code.clone()));
        }
        children.push(detail_row(
            fl!("activity-detail-counts"),
            compact_value(&op.counts),
        ));

        children.push(widget::divider::horizontal::default().into());

        if let Some(message) = &selection.error {
            children.push(
                widget::text::body(fl!("activity-error", message = message.as_str()))
                    .class(cosmic::theme::Text::Color(Color::from_rgb8(
                        0xC6, 0x2E, 0x2E,
                    )))
                    .into(),
            );
        } else {
            match &selection.detail {
                None => children.push(widget::text::body(fl!("activity-loading")).into()),
                Some(detail) => {
                    children.push(widget::text::heading(fl!("activity-detail-events")).into());
                    if detail.events.is_empty() {
                        children.push(
                            widget::text::body(fl!("activity-detail-no-events"))
                                .class(cosmic::theme::Text::Color(DIM))
                                .into(),
                        );
                    } else {
                        for event in &detail.events {
                            children.push(self.event_card(event));
                        }
                    }
                }
            }
        }

        if !selection.related.is_empty() {
            children.push(widget::divider::horizontal::default().into());
            children.push(widget::text::heading(fl!("activity-detail-related")).into());
            for detail in &selection.related {
                children.push(widget::text::body(operation_summary(&detail.operation)).into());
            }
        }

        widget::scrollable(widget::column::with_children(children).spacing(12)).into()
    }

    fn event_card(&self, event: &ActivityEvent) -> Element<'_, ActivityMessage> {
        let mut children: Vec<Element<'_, ActivityMessage>> = vec![
            widget::Row::with_children(vec![
                widget::text::body(event.event_type.as_str())
                    .size(12)
                    .class(cosmic::theme::Text::Color(DIM))
                    .into(),
                widget::text::body(outcome_label(event.outcome)).into(),
                widget::text::body(friendly_when(event.occurred_at.as_str()))
                    .size(11)
                    .class(cosmic::theme::Text::Color(DIM))
                    .into(),
            ])
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
        ];
        if !matches!(event.parameters.as_object(), Some(map) if map.is_empty()) {
            children.push(
                widget::text::body(compact_value(&event.parameters))
                    .size(11)
                    .class(cosmic::theme::Text::Color(DIM))
                    .into(),
            );
        }
        if !event.targets.is_empty() {
            children.push(
                widget::text::body(
                    event
                        .targets
                        .iter()
                        .map(target_label)
                        .collect::<Vec<String>>()
                        .join(", "),
                )
                .size(11)
                .class(cosmic::theme::Text::Color(DIM))
                .into(),
            );
        }
        widget::container(widget::column::with_children(children).spacing(4))
            .padding(8)
            .width(Length::Fill)
            .style(move |_theme: &cosmic::Theme| widget::container::background(CARD_BG))
            .into()
    }
}

// ── presenters ──────────────────────────────────────────────────────────────

fn detail_row(label: String, value: String) -> Element<'static, ActivityMessage> {
    widget::column::with_children(vec![
        widget::text::body(label)
            .size(11)
            .class(cosmic::theme::Text::Color(DIM))
            .into(),
        widget::text::monotext(value).into(),
    ])
    .spacing(2)
    .into()
}

fn operation_type_label(kind: OperationType) -> String {
    match kind {
        OperationType::Scan => fl!("activity-op-scan"),
        OperationType::FileDelete => fl!("activity-op-file-delete"),
        OperationType::DocumentMerge => fl!("activity-op-document-merge"),
        OperationType::MetadataEdit => fl!("activity-op-metadata-edit"),
        OperationType::TagEdit => fl!("activity-op-tag-edit"),
        OperationType::StatusEdit => fl!("activity-op-status-edit"),
        OperationType::CoverEdit => fl!("activity-op-cover-edit"),
        OperationType::MissingFileMaintenance => fl!("activity-op-missing-file-maintenance"),
    }
}

fn channel_label(channel: AuditChannel) -> String {
    match channel {
        AuditChannel::Rest => fl!("activity-channel-rest"),
        AuditChannel::Cosmic => fl!("activity-channel-cosmic"),
        AuditChannel::Cli => fl!("activity-channel-cli"),
        AuditChannel::Internal => fl!("activity-channel-internal"),
    }
}

fn actor_label(actor: &AuditActor) -> String {
    match actor.kind {
        AuditActorKind::User => {
            fl!(
                "activity-actor-user",
                id = actor.id.as_deref().unwrap_or_default()
            )
        }
        AuditActorKind::LocalIdentity => fl!("activity-actor-local"),
        AuditActorKind::LocalSession => fl!("activity-actor-local-session"),
        AuditActorKind::System => fl!("activity-actor-system"),
    }
}

fn status_label(status: OperationStatus) -> String {
    match status {
        OperationStatus::Started => fl!("activity-status-started"),
        OperationStatus::Completed => fl!("activity-status-completed"),
        OperationStatus::Failed => fl!("activity-status-failed"),
        OperationStatus::Interrupted => fl!("activity-status-interrupted"),
    }
}

fn outcome_label(outcome: AuditOutcome) -> String {
    match outcome {
        AuditOutcome::Success => fl!("activity-outcome-success"),
        AuditOutcome::CompletedWithErrors => fl!("activity-outcome-completed-with-errors"),
        AuditOutcome::Failed => fl!("activity-outcome-failed"),
        AuditOutcome::Observed => fl!("activity-outcome-observed"),
        AuditOutcome::Proposed => fl!("activity-outcome-proposed"),
    }
}

/// Localized one-line summary of an operation, derived from its type + counts.
fn operation_summary(op: &ActivityOperation) -> String {
    let counts = compact_value(&op.counts);
    match op.operation_type {
        OperationType::Scan => fl!("activity-summary-scan", counts = counts.as_str()),
        OperationType::FileDelete => fl!("activity-summary-file-delete"),
        OperationType::DocumentMerge => fl!("activity-summary-document-merge"),
        OperationType::MetadataEdit => fl!("activity-summary-metadata-edit"),
        OperationType::TagEdit => fl!("activity-summary-tag-edit"),
        OperationType::StatusEdit => fl!("activity-summary-status-edit"),
        OperationType::CoverEdit => fl!("activity-summary-cover-edit"),
        OperationType::MissingFileMaintenance => fl!(
            "activity-summary-missing-file-maintenance",
            counts = counts.as_str()
        ),
    }
}

/// A short human-readable rendering of a snapshot target.
fn target_label(target: &ActivityTarget) -> String {
    target
        .title_snapshot
        .as_deref()
        .or(target.path_snapshot.as_deref())
        .or(target.fingerprint_snapshot.as_deref())
        .unwrap_or(&target.target_id)
        .to_string()
}

/// Render a JSON object compactly, e.g. `"tags_added = 2"`; unknown values
/// render as-is.
fn compact_value(value: &Value) -> String {
    match value {
        Value::Object(map) if map.is_empty() => "{}".to_string(),
        Value::Object(map) => map
            .iter()
            .map(|(k, v)| match v {
                Value::Number(n) => format!("{k} = {n}"),
                Value::String(s) => format!("{k} = {s}"),
                Value::Bool(b) => format!("{k} = {b}"),
                other => format!("{k} = {other}"),
            })
            .collect::<Vec<_>>()
            .join(", "),
        other => other.to_string(),
    }
}

/// Render a Unix-microseconds timestamp (the activity store's wire format) as
/// a wall-clock `YYYY-MM-DD HH:MM` string in the system's local time. Falls
/// back to the raw value when it cannot be parsed or no local offset applies.
fn friendly_when(when: &str) -> String {
    let Ok(micros) = when.parse::<i64>() else {
        return when.to_string();
    };
    let offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
    format_when_at_offset(micros, offset).unwrap_or_else(|| when.to_string())
}

/// Deterministic formatting of a Unix-microseconds timestamp at a fixed offset.
fn format_when_at_offset(micros: i64, offset: UtcOffset) -> Option<String> {
    let utc = OffsetDateTime::from_unix_timestamp(micros.div_euclid(1_000_000)).ok()?
        + Duration::microseconds(micros.rem_euclid(1_000_000));
    utc.to_offset(offset)
        .format(format_description!("[year]-[month]-[day] [hour]:[minute]"))
        .ok()
}

/// A solid-color rounded background style.
fn rounded_bg(color: Color, radius: f32) -> widget::container::Style {
    use cosmic::iced::Border;
    let mut style = widget::container::background(color);
    style.border = Border {
        radius: radius.into(),
        ..Default::default()
    };
    style
}

#[cfg(test)]
mod tests {
    use time::UtcOffset;

    use super::format_when_at_offset;
    use super::friendly_when;

    /// 2026-09-08T12:34:56Z in Unix microseconds.
    const MICROS: i64 = 1_788_870_896_000_000;

    fn offset(seconds: i32) -> UtcOffset {
        UtcOffset::from_whole_seconds(seconds).expect("valid offset")
    }

    #[test]
    fn formats_micros_timestamp_at_utc() {
        assert_eq!(
            format_when_at_offset(MICROS, UtcOffset::UTC),
            Some("2026-09-08 12:34".to_string())
        );
    }

    #[test]
    fn formats_micros_timestamp_at_a_local_offset() {
        assert_eq!(
            format_when_at_offset(MICROS, offset(2 * 3600)),
            Some("2026-09-08 14:34".to_string())
        );
    }

    #[test]
    fn formats_micros_timestamp_with_subsecond_micros() {
        assert_eq!(
            format_when_at_offset(MICROS + 123_456, UtcOffset::UTC),
            Some("2026-09-08 12:34".to_string())
        );
    }

    #[test]
    fn friendly_when_passes_through_garbage() {
        assert_eq!(friendly_when("not-a-timestamp"), "not-a-timestamp");
    }
}
