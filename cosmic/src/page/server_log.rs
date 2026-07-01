// SPDX-License-Identifier: GPL-3.0-or-later
//! Server control + live log page.
//!
//! Shows the embedded HTTP server's status, start/stop/restart/reload controls,
//! and a filterable/searchable view of the captured JSON log ([`LogBus`]).
//! Server control lives in the App (it owns the process handle); this page only
//! renders status and emits [`ServerLogOutput`] for the App to act on.

use std::net::SocketAddr;

use cosmic::Action;
use cosmic::Apply;
use cosmic::Element;
use cosmic::Task;
use cosmic::iced::Alignment;
use cosmic::iced::Length;
use cosmic::widget;
use tracing::Level;

use crate::fl;
use crate::logging::LogBus;
use crate::logging::LogEntry;
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

/// Level filter choices, in dropdown order. `None` = all levels.
const LEVELS: [Option<Level>; 6] = [
    None,
    Some(Level::ERROR),
    Some(Level::WARN),
    Some(Level::INFO),
    Some(Level::DEBUG),
    Some(Level::TRACE),
];

/// Max rows rendered at once (the buffer can hold many more).
const MAX_RENDERED: usize = 1000;

#[derive(Debug, Clone)]
pub enum ServerLogOutput {
    Start,
    Stop,
    Restart,
    ReloadConfig,
}

#[derive(Debug, Clone)]
pub enum ServerLogMessage {
    /// The log buffer changed; re-read and re-filter.
    LogsChanged,
    /// The App pushed a new server status.
    StatusChanged(ServerStatus),
    SearchChanged(String),
    LevelSelected(usize),
    Out(ServerLogOutput),
}

pub struct ServerLogPage {
    log_bus: LogBus,
    status: ServerStatus,
    search: String,
    level_index: usize,
    /// Cached, already-filtered view (most recent last).
    filtered: Vec<LogEntry>,
}

impl ServerLogPage {
    pub fn new(log_bus: LogBus) -> Self {
        let mut page = Self {
            log_bus,
            status: ServerStatus::Stopped,
            search: String::new(),
            level_index: 0,
            filtered: Vec::new(),
        };
        page.recompute();
        page
    }

    fn min_level(&self) -> Option<Level> {
        LEVELS.get(self.level_index).copied().flatten()
    }

    fn matches(&self, entry: &LogEntry) -> bool {
        // `tracing::Level` orders ERROR < WARN < INFO < ... via `>=` on verbosity;
        // "at least this severe" means `entry.level <= min` (ERROR is most severe).
        if let Some(min) = self.min_level()
            && entry.level > min
        {
            return false;
        }
        if self.search.is_empty() {
            return true;
        }
        let needle = self.search.to_lowercase();
        entry.message.to_lowercase().contains(&needle)
            || entry.target.to_lowercase().contains(&needle)
            || entry.fields_summary().to_lowercase().contains(&needle)
    }

    fn recompute(&mut self) {
        let snapshot = self.log_bus.snapshot();
        self.filtered = snapshot.into_iter().filter(|e| self.matches(e)).collect();
        let len = self.filtered.len();
        if len > MAX_RENDERED {
            self.filtered.drain(0..len - MAX_RENDERED);
        }
    }

    fn level_label(level: Level) -> &'static str {
        match level {
            Level::ERROR => "ERROR",
            Level::WARN => "WARN",
            Level::INFO => "INFO",
            Level::DEBUG => "DEBUG",
            Level::TRACE => "TRACE",
        }
    }

    fn status_text(&self) -> String {
        match &self.status {
            ServerStatus::Stopped => fl!("server-status-stopped"),
            ServerStatus::Starting => fl!("server-status-starting"),
            ServerStatus::Running(addr) => {
                fl!("server-status-running", address = format!("http://{addr}"))
            }
            ServerStatus::Failed(err) => fl!("server-status-failed", error = err.clone()),
        }
    }

    fn controls(&self) -> Element<'_, ServerLogMessage> {
        let running = self.status.is_running();
        let busy = self.status.is_busy();

        let start = {
            let b = widget::button::standard(fl!("server-start"));
            if !running && !busy {
                b.on_press(ServerLogMessage::Out(ServerLogOutput::Start))
            } else {
                b
            }
        };
        let stop = {
            let b = widget::button::standard(fl!("server-stop"));
            if running {
                b.on_press(ServerLogMessage::Out(ServerLogOutput::Stop))
            } else {
                b
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

        widget::Row::with_children(vec![
            widget::text::heading(self.status_text()).into(),
            widget::space::horizontal().width(Length::Fill).into(),
            start.into(),
            stop.into(),
            restart.into(),
            reload.into(),
        ])
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    }

    fn filters(&self) -> Element<'_, ServerLogMessage> {
        let level_options: Vec<String> = LEVELS
            .iter()
            .map(|l| match l {
                None => fl!("server-log-level-all"),
                Some(level) => Self::level_label(*level).to_string(),
            })
            .collect();

        widget::Row::with_children(vec![
            widget::text_input(fl!("server-log-search"), &self.search)
                .on_input(ServerLogMessage::SearchChanged)
                .width(Length::Fill)
                .into(),
            widget::dropdown(
                level_options,
                Some(self.level_index),
                ServerLogMessage::LevelSelected,
            )
            .into(),
        ])
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    }

    fn log_list(&self) -> Element<'_, ServerLogMessage> {
        let mut children: Vec<Element<'_, ServerLogMessage>> = Vec::new();
        if self.filtered.is_empty() {
            children.push(widget::text::body(fl!("server-log-empty")).into());
        } else {
            for entry in &self.filtered {
                let line = format!(
                    "{} {:>5} {}: {} {}",
                    entry.timestamp,
                    Self::level_label(entry.level),
                    entry.target,
                    entry.message,
                    entry.fields_summary(),
                );
                children.push(widget::text::monotext(line).size(12).into());
            }
        }
        widget::column::with_children(children)
            .spacing(2)
            .apply(widget::scrollable::vertical)
            .height(Length::Fill)
            .into()
    }
}

impl Page for ServerLogPage {
    type Message = ServerLogMessage;

    fn view(&self) -> Element<'_, Self::Message> {
        widget::column::with_children(vec![self.controls(), self.filters(), self.log_list()])
            .spacing(12)
            .padding(16)
            .into()
    }

    fn update(&mut self, message: Self::Message) -> Task<Action<Self::Message>> {
        match message {
            ServerLogMessage::LogsChanged => self.recompute(),
            ServerLogMessage::StatusChanged(status) => self.status = status,
            ServerLogMessage::SearchChanged(search) => {
                self.search = search;
                self.recompute();
            }
            ServerLogMessage::LevelSelected(index) => {
                self.level_index = index;
                self.recompute();
            }
            // `Out` is intercepted by the message mapper at the view boundary and
            // never reaches here; the arm exists only for exhaustiveness.
            ServerLogMessage::Out(_) => {}
        }
        Task::none()
    }
}
