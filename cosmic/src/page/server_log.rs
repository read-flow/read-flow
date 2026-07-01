// SPDX-License-Identifier: GPL-3.0-or-later
//! Server control + live log page.
//!
//! A self-explanatory control panel for the embedded HTTP server, plus a
//! colorized, filterable log. Rows are clickable: selecting one opens the
//! context pane with the full message, structured fields, and enclosing spans.
//!
//! Server control lives in the App (it owns the process handle); this page only
//! renders status and emits [`ServerLogOutput`] for the App to act on.

use std::net::SocketAddr;

use cosmic::Action;
use cosmic::Apply;
use cosmic::Element;
use cosmic::Task;
use cosmic::iced::Alignment;
use cosmic::iced::Border;
use cosmic::iced::Color;
use cosmic::iced::Length;
use cosmic::task;
use cosmic::widget;
use tracing::Level;

use crate::app::ContextView;
use crate::fl;
use crate::logging::LogBus;
use crate::logging::LogEntry;
use crate::logging::SpanInfo;
use crate::logging::render_value;
use crate::page::traits::Page;

/// Runtime status of the embedded server, mirrored into the App model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerStatus {
    Stopped,
    Starting,
    Running(SocketAddr),
    Failed(String),
}

impl ServerStatus {
    fn is_running(&self) -> bool {
        matches!(self, ServerStatus::Running(_))
    }
    fn is_busy(&self) -> bool {
        matches!(self, ServerStatus::Starting)
    }
}

/// Max rows rendered at once (the buffer can hold many more).
const MAX_RENDERED: usize = 1000;

/// Severity rank, high = more severe. Used for the min-level filter.
fn severity(level: Level) -> usize {
    match level {
        Level::TRACE => 0,
        Level::DEBUG => 1,
        Level::INFO => 2,
        Level::WARN => 3,
        Level::ERROR => 4,
    }
}

fn level_from_severity(sev: usize) -> Level {
    match sev {
        0 => Level::TRACE,
        1 => Level::DEBUG,
        2 => Level::INFO,
        3 => Level::WARN,
        _ => Level::ERROR,
    }
}

fn level_label(level: Level) -> String {
    match level {
        Level::TRACE => fl!("log-level-trace"),
        Level::DEBUG => fl!("log-level-debug"),
        Level::INFO => fl!("log-level-info"),
        Level::WARN => fl!("log-level-warn"),
        Level::ERROR => fl!("log-level-error"),
    }
}

fn level_short(level: Level) -> &'static str {
    match level {
        Level::TRACE => "TRACE",
        Level::DEBUG => "DEBUG",
        Level::INFO => "INFO",
        Level::WARN => "WARN",
        Level::ERROR => "ERROR",
    }
}

/// Chip background color per level (chosen to read on both light and dark).
fn level_color(level: Level) -> Color {
    match level {
        Level::TRACE => Color::from_rgb8(0x8A, 0x8A, 0x8A),
        Level::DEBUG => Color::from_rgb8(0x5B, 0x8D, 0xB5),
        Level::INFO => Color::from_rgb8(0x2E, 0x8B, 0x57),
        Level::WARN => Color::from_rgb8(0xD9, 0x8E, 0x1F),
        Level::ERROR => Color::from_rgb8(0xD1, 0x3B, 0x3B),
    }
}

/// Message tint: only WARN/ERROR get emphasized so normal logs stay calm.
fn emphasis_color(level: Level) -> Option<Color> {
    match level {
        Level::WARN => Some(Color::from_rgb8(0xB8, 0x76, 0x0E)),
        Level::ERROR => Some(Color::from_rgb8(0xC6, 0x2E, 0x2E)),
        _ => None,
    }
}

const DIM: Color = Color::from_rgb(0.55, 0.55, 0.55);
const SELECTED_BG: Color = Color::from_rgba(0.40, 0.60, 1.0, 0.14);
const SPAN_BG: Color = Color::from_rgba(0.5, 0.5, 0.5, 0.08);

/// Last path segment of a target (`read_flow_core::server` → `server`).
fn short_target(target: &str) -> &str {
    target.rsplit("::").next().unwrap_or(target)
}

/// Shorten a string to `max` chars by eliding the middle (`/documents/1a2…/cover`),
/// keeping the ends readable. The full value stays searchable and in the details.
fn truncate_middle(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        return s.to_string();
    }
    let keep = max.saturating_sub(1); // room for the ellipsis
    let head = keep.div_ceil(2);
    let tail = keep - head;
    let head_s: String = chars[..head].iter().collect();
    let tail_s: String = chars[chars.len() - tail..].iter().collect();
    format!("{head_s}…{tail_s}")
}

#[derive(Debug, Clone)]
pub enum ServerLogOutput {
    Start,
    Stop,
    Restart,
    ReloadConfig,
    OpenContext,
    CloseContext,
}

#[derive(Debug, Clone)]
pub enum ServerLogMessage {
    /// The log buffer changed; re-read and re-filter.
    LogsChanged,
    /// The App pushed a new server status.
    StatusChanged(ServerStatus),
    SearchChanged(String),
    MinLevelSelected(usize),
    EntrySelected(u64),
    ClearSelection,
    Out(ServerLogOutput),
}

pub struct ServerLogPage {
    log_bus: LogBus,
    status: ServerStatus,
    search: String,
    /// Minimum severity shown (INFO by default): this level and everything above.
    min_severity: usize,
    /// Cached, already-filtered view (most recent last).
    filtered: Vec<LogEntry>,
    /// The entry whose details are shown in the context pane.
    selected: Option<LogEntry>,
}

impl ServerLogPage {
    pub fn new(log_bus: LogBus) -> Self {
        let mut page = Self {
            log_bus,
            status: ServerStatus::Stopped,
            search: String::new(),
            min_severity: severity(Level::INFO),
            filtered: Vec::new(),
            selected: None,
        };
        page.recompute();
        page
    }

    fn matches(&self, entry: &LogEntry) -> bool {
        if severity(entry.level) < self.min_severity {
            return false;
        }
        if self.search.is_empty() {
            return true;
        }
        let needle = self.search.to_lowercase();
        entry.message.to_lowercase().contains(&needle)
            || entry.target.to_lowercase().contains(&needle)
            || entry.fields_summary().to_lowercase().contains(&needle)
            || entry
                .uri()
                .is_some_and(|u| u.to_lowercase().contains(&needle))
            || entry
                .method()
                .is_some_and(|m| m.to_lowercase().contains(&needle))
    }

    fn recompute(&mut self) {
        let snapshot = self.log_bus.snapshot();
        self.filtered = snapshot.into_iter().filter(|e| self.matches(e)).collect();
        let len = self.filtered.len();
        if len > MAX_RENDERED {
            self.filtered.drain(0..len - MAX_RENDERED);
        }
        if let Some(selected) = &self.selected
            && let Some(updated) = self.filtered.iter().find(|e| e.id == selected.id)
        {
            self.selected = Some(updated.clone());
        }
    }

    fn error_warn_counts(&self) -> (usize, usize) {
        self.filtered
            .iter()
            .fold((0, 0), |(e, w), entry| match entry.level {
                Level::ERROR => (e + 1, w),
                Level::WARN => (e, w + 1),
                _ => (e, w),
            })
    }

    // ── Control panel ────────────────────────────────────────────────────────

    /// Colored status indicator, human sentence, and detail line.
    fn status_summary(&self) -> (Color, String, Option<String>) {
        match &self.status {
            ServerStatus::Stopped => (
                DIM,
                fl!("server-status-stopped"),
                Some(fl!("server-status-stopped-detail")),
            ),
            ServerStatus::Starting => (
                level_color(Level::WARN),
                fl!("server-status-starting"),
                None,
            ),
            ServerStatus::Running(addr) => (
                level_color(Level::INFO),
                fl!("server-status-running"),
                Some(fl!(
                    "server-status-running-detail",
                    address = format!("http://{addr}")
                )),
            ),
            ServerStatus::Failed(err) => (
                level_color(Level::ERROR),
                fl!("server-status-failed"),
                Some(err.clone()),
            ),
        }
    }

    fn control_panel(&self) -> Element<'_, ServerLogMessage> {
        let (dot_color, title, detail) = self.status_summary();

        let mut text_children: Vec<Element<'_, ServerLogMessage>> =
            vec![widget::text::heading(title).into()];
        if let Some(detail) = detail {
            text_children.push(
                widget::text::body(detail)
                    .class(cosmic::theme::Text::Color(DIM))
                    .into(),
            );
        }
        let text_col = widget::column::with_children(text_children).spacing(2);

        let status_row = widget::Row::with_children(vec![status_dot(dot_color), text_col.into()])
            .spacing(12)
            .align_y(Alignment::Center);

        let running = self.status.is_running();
        let busy = self.status.is_busy();

        // Primary action flips between Start and Stop depending on state.
        let primary = if running {
            widget::button::destructive(fl!("server-stop"))
                .on_press(ServerLogMessage::Out(ServerLogOutput::Stop))
        } else {
            let b = widget::button::suggested(fl!("server-start"));
            if busy {
                b
            } else {
                b.on_press(ServerLogMessage::Out(ServerLogOutput::Start))
            }
        };

        let restart = {
            let b = widget::button::standard(fl!("server-restart"));
            if running {
                b.on_press(ServerLogMessage::Out(ServerLogOutput::Restart))
            } else {
                b
            }
        };
        let reload = {
            let b = widget::button::standard(fl!("server-reload-config"));
            if running {
                b.on_press(ServerLogMessage::Out(ServerLogOutput::ReloadConfig))
            } else {
                b
            }
        };

        let buttons =
            widget::Row::with_children(vec![primary.into(), restart.into(), reload.into()])
                .spacing(8);

        widget::settings::section()
            .title(fl!("server-panel-title"))
            .add(status_row)
            .add(buttons)
            .into()
    }

    // ── Filter bar ───────────────────────────────────────────────────────────

    fn filter_bar(&self) -> Element<'_, ServerLogMessage> {
        let level_options: Vec<String> = (0..=4)
            .map(|s| level_label(level_from_severity(s)))
            .collect();

        let (errors, warns) = self.error_warn_counts();
        let counts =
            widget::text::body(fl!("server-log-counts", errors = errors, warnings = warns))
                .class(cosmic::theme::Text::Color(DIM));

        widget::Row::with_children(vec![
            widget::text::body(fl!("server-log-min-level")).into(),
            widget::dropdown(
                level_options,
                Some(self.min_severity),
                ServerLogMessage::MinLevelSelected,
            )
            .into(),
            widget::text_input(fl!("server-log-search"), &self.search)
                .on_input(ServerLogMessage::SearchChanged)
                .width(Length::Fill)
                .into(),
            counts.into(),
        ])
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    }

    // ── Log list ─────────────────────────────────────────────────────────────

    fn log_row(&self, entry: &LogEntry) -> Element<'_, ServerLogMessage> {
        let selected = self.selected.as_ref().map(|e| e.id) == Some(entry.id);

        let time = widget::text::monotext(entry.timestamp.clone())
            .size(12)
            .class(cosmic::theme::Text::Color(DIM))
            .width(Length::Fixed(92.0));

        let target = widget::text::body(short_target(&entry.target).to_string())
            .size(12)
            .class(cosmic::theme::Text::Color(DIM))
            .width(Length::Fixed(120.0));

        let method = widget::text::monotext(entry.method().unwrap_or_default())
            .size(12)
            .class(cosmic::theme::Text::Color(DIM))
            .width(Length::Fixed(52.0));

        let uri = widget::text::monotext(
            entry
                .uri()
                .map(|u| truncate_middle(&u, 24))
                .unwrap_or_default(),
        )
        .size(12)
        .class(cosmic::theme::Text::Color(DIM))
        .width(Length::Fixed(180.0));

        let mut message = widget::text::body(entry.message.clone()).size(13);
        if let Some(color) = emphasis_color(entry.level) {
            message = message.class(cosmic::theme::Text::Color(color));
        }

        let row = widget::Row::with_children(vec![
            time.into(),
            level_chip(entry.level),
            target.into(),
            method.into(),
            uri.into(),
            message.width(Length::Fill).into(),
        ])
        .spacing(8)
        .align_y(Alignment::Center);

        let mut container = widget::container(row).padding([3, 6]).width(Length::Fill);
        if selected {
            container = container.style(move |_theme: &cosmic::Theme| rounded_bg(SELECTED_BG, 4.0));
        }

        widget::mouse_area(container)
            .on_press(ServerLogMessage::EntrySelected(entry.id))
            .into()
    }

    fn log_list(&self) -> Element<'_, ServerLogMessage> {
        if self.filtered.is_empty() {
            return widget::container(
                widget::text::body(fl!("server-log-empty")).class(cosmic::theme::Text::Color(DIM)),
            )
            .center_x(Length::Fill)
            .padding(24)
            .into();
        }

        let children: Vec<Element<'_, ServerLogMessage>> =
            self.filtered.iter().map(|e| self.log_row(e)).collect();
        widget::column::with_children(children)
            .spacing(1)
            .apply(widget::scrollable::vertical)
            .height(Length::Fill)
            .into()
    }

    // ── Context pane (entry details) ─────────────────────────────────────────

    fn entry_details(&self, entry: &LogEntry) -> Element<'_, ServerLogMessage> {
        let back =
            widget::button::link(fl!("server-log-back")).on_press(ServerLogMessage::ClearSelection);

        let header = widget::Row::with_children(vec![
            level_chip(entry.level),
            widget::text::monotext(entry.timestamp.clone())
                .class(cosmic::theme::Text::Color(DIM))
                .into(),
        ])
        .spacing(8)
        .align_y(Alignment::Center);

        let mut children: Vec<Element<'_, ServerLogMessage>> = vec![
            back.into(),
            header.into(),
            detail_row(fl!("server-log-detail-target"), entry.target.clone()),
            detail_row(fl!("server-log-detail-message"), entry.message.clone()),
        ];

        if !entry.fields.is_empty() {
            children.push(widget::divider::horizontal::default().into());
            children.push(widget::text::heading(fl!("server-log-detail-fields")).into());
            for (key, value) in &entry.fields {
                children.push(detail_row(key.clone(), render_value(value)));
            }
        }

        if !entry.spans.is_empty() {
            children.push(widget::divider::horizontal::default().into());
            children.push(widget::text::heading(fl!("server-log-detail-spans")).into());
            for span in &entry.spans {
                children.push(span_card(span));
            }
        }

        widget::column::with_children(children).spacing(12).into()
    }
}

impl Page for ServerLogPage {
    type Message = ServerLogMessage;

    fn view(&self) -> Element<'_, Self::Message> {
        widget::column::with_children(vec![
            self.control_panel(),
            self.filter_bar(),
            self.log_list(),
        ])
        .spacing(12)
        .padding(16)
        .into()
    }

    fn view_context(&self) -> ContextView<'_, Self::Message> {
        let content = match &self.selected {
            Some(entry) => self.entry_details(entry),
            None => widget::text::body(fl!("server-log-select-hint")).into(),
        };
        ContextView {
            title: fl!("server-log-details-title"),
            content,
        }
    }

    fn update(&mut self, message: Self::Message) -> Task<Action<Self::Message>> {
        match message {
            ServerLogMessage::LogsChanged => self.recompute(),
            ServerLogMessage::StatusChanged(status) => self.status = status,
            ServerLogMessage::SearchChanged(search) => {
                self.search = search;
                self.recompute();
            }
            ServerLogMessage::MinLevelSelected(severity) => {
                self.min_severity = severity;
                self.recompute();
            }
            ServerLogMessage::EntrySelected(id) => {
                self.selected = self.filtered.iter().find(|e| e.id == id).cloned();
                // Ask the App to open the context drawer for this page.
                return task::message(ServerLogMessage::Out(ServerLogOutput::OpenContext));
            }
            ServerLogMessage::ClearSelection => {
                self.selected = None;
                return task::message(ServerLogMessage::Out(ServerLogOutput::CloseContext));
            }
            // Other `Out` variants are intercepted by the mapper at the view
            // boundary and never reach here.
            ServerLogMessage::Out(_) => {}
        }
        Task::none()
    }
}

// ── small view helpers ───────────────────────────────────────────────────────

/// A solid-color rounded background style.
fn rounded_bg(color: Color, radius: f32) -> widget::container::Style {
    let mut style = widget::container::background(color);
    style.border = Border {
        radius: radius.into(),
        ..Default::default()
    };
    style
}

/// A small colored square, the server status indicator.
fn status_dot<'a>(color: Color) -> Element<'a, ServerLogMessage> {
    widget::container(widget::text::body(""))
        .width(Length::Fixed(12.0))
        .height(Length::Fixed(12.0))
        .style(move |_theme: &cosmic::Theme| rounded_bg(color, 6.0))
        .into()
}

/// A colored level chip with white text.
fn level_chip<'a>(level: Level) -> Element<'a, ServerLogMessage> {
    let color = level_color(level);
    widget::container(
        widget::text::body(level_short(level))
            .size(11)
            .class(cosmic::theme::Text::Color(Color::WHITE)),
    )
    .padding([1, 6])
    .style(move |_theme: &cosmic::Theme| rounded_bg(color, 4.0))
    .into()
}

/// A "Label / value" detail block for the context pane.
fn detail_row<'a>(label: String, value: String) -> Element<'a, ServerLogMessage> {
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

/// A card showing one span's name, target, and fields.
fn span_card<'a>(span: &SpanInfo) -> Element<'a, ServerLogMessage> {
    let header = widget::Row::with_children(vec![
        widget::text::heading(span.name.clone()).into(),
        widget::text::body(short_target(&span.target).to_string())
            .size(11)
            .class(cosmic::theme::Text::Color(DIM))
            .into(),
    ])
    .spacing(8)
    .align_y(Alignment::Center);

    let mut children: Vec<Element<'_, ServerLogMessage>> = vec![header.into()];
    for (key, value) in &span.fields {
        children.push(
            widget::text::monotext(format!("{key} = {}", render_value(value)))
                .size(12)
                .into(),
        );
    }

    widget::container(widget::column::with_children(children).spacing(2))
        .padding(8)
        .width(Length::Fill)
        .style(move |_theme: &cosmic::Theme| rounded_bg(SPAN_BG, 6.0))
        .into()
}
