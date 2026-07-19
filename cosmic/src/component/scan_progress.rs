// SPDX-License-Identifier: AGPL-3.0-or-later

use cosmic::cosmic_theme;
use cosmic::prelude::*;
use cosmic::task;
use cosmic::theme;
use cosmic::widget;
use cosmic::widget::icon;
use read_flow_core::scan::ScanProgress;

use crate::fl;

#[derive(Debug, Clone)]
pub enum ScanProgressOutput {
    /// Scan stream fully drained — parent should expire the document provider.
    Completed,
    /// User dismissed the footer — parent should set scan_component = None.
    Dismissed,
}

#[derive(Debug, Clone)]
pub enum ScanProgressMessage {
    Progress(ScanProgress),
    Completed,
    Dismiss,
    /// Open the scan report dialog (click on the "Last scan: ..." footer).
    ViewReport,
    /// Close the scan report dialog.
    CloseReport,
    Out(ScanProgressOutput),
}

/// @feature: admin.scan
pub struct ScanComponent {
    discovered: u64,
    processed: u64,
    errors: u64,
    added: u64,
    updated: u64,
    error_details: Vec<(String, String)>,
    active: bool,
    report_open: bool,
}

impl ScanComponent {
    pub fn new() -> Self {
        Self {
            discovered: 0,
            processed: 0,
            errors: 0,
            added: 0,
            updated: 0,
            error_details: Vec::new(),
            active: true,
            report_open: false,
        }
    }

    pub fn update(
        &mut self,
        message: ScanProgressMessage,
    ) -> Task<cosmic::Action<ScanProgressMessage>> {
        match message {
            ScanProgressMessage::Progress(event) => {
                match event {
                    ScanProgress::FileDiscovered => self.discovered += 1,
                    ScanProgress::FileProcessed {
                        was_new,
                        was_updated,
                        ..
                    } => {
                        self.processed += 1;
                        if was_new {
                            self.added += 1;
                        } else if was_updated {
                            self.updated += 1;
                        }
                    }
                    ScanProgress::FileError { path, error } => {
                        self.errors += 1;
                        self.error_details.push((path.display().to_string(), error));
                    }
                    ScanProgress::Completed { .. } => {}
                }
                Task::none()
            }
            ScanProgressMessage::Completed => {
                self.active = false;
                task::message(ScanProgressMessage::Out(ScanProgressOutput::Completed))
            }
            ScanProgressMessage::Dismiss => {
                task::message(ScanProgressMessage::Out(ScanProgressOutput::Dismissed))
            }
            ScanProgressMessage::ViewReport => {
                self.report_open = true;
                Task::none()
            }
            ScanProgressMessage::CloseReport => {
                self.report_open = false;
                Task::none()
            }
            ScanProgressMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent")
            }
        }
    }

    /// Report dialog shown after a scan completes, once opened via `ViewReport`.
    pub fn dialog(&self) -> Option<Element<'_, ScanProgressMessage>> {
        if self.active || !self.report_open {
            return None;
        }

        let cosmic_theme::Spacing {
            space_s, space_xs, ..
        } = theme::active().cosmic().spacing;

        let dialog = if self.error_details.is_empty() {
            widget::dialog()
                .title(fl!("scan-report-title"))
                .body(fl!(
                    "scan-report-summary",
                    added = self.added,
                    updated = self.updated,
                    errors = self.errors
                ))
                .icon(icon::from_name("dialog-information-symbolic").size(64))
                .primary_action(
                    widget::button::suggested(fl!("scan-report-close"))
                        .on_press(ScanProgressMessage::CloseReport),
                )
        } else {
            let error_list = widget::column::with_children(
                self.error_details
                    .iter()
                    .map(|(path, message)| {
                        widget::text::monotext(format!("{path}: {message}")).into()
                    })
                    .collect::<Vec<_>>(),
            )
            .spacing(space_xs);

            widget::dialog()
                .title(fl!("scan-report-title"))
                .body(fl!(
                    "scan-report-summary",
                    added = self.added,
                    updated = self.updated,
                    errors = self.errors
                ))
                .icon(icon::from_name("dialog-warning-symbolic").size(64))
                .control(
                    error_list
                        .apply(widget::scrollable::vertical)
                        .height(cosmic::iced::Length::Fixed(300.0))
                        .apply(widget::container)
                        .class(cosmic::theme::Container::Card)
                        .padding(space_s)
                        .width(cosmic::iced::Length::Fill),
                )
                .primary_action(
                    widget::button::suggested(fl!("scan-report-close"))
                        .on_press(ScanProgressMessage::CloseReport),
                )
        };

        Some(dialog.into())
    }

    pub fn view(&self) -> Element<'_, ScanProgressMessage> {
        let theme = cosmic::theme::active();
        let spacing = theme.cosmic().space_s();
        let padding = theme.cosmic().space_xs();

        let (label_element, progress): (Element<'_, ScanProgressMessage>, f32) = if self.active {
            let label = fl!(
                "scan-progress-scanning",
                discovered = self.discovered,
                processed = self.processed
            );
            let max = self.discovered.max(1) as f32;
            (
                widget::text(label)
                    .width(cosmic::iced::Length::Fixed(400.0))
                    .into(),
                self.processed as f32 / max,
            )
        } else {
            let label = fl!(
                "scan-progress-completed",
                discovered = self.discovered,
                processed = self.processed,
                errors = self.errors
            );
            let label_element = widget::text(label)
                .width(cosmic::iced::Length::Fixed(400.0))
                .apply(widget::mouse_area)
                .on_press(ScanProgressMessage::ViewReport)
                .into();
            (label_element, 1.0_f32)
        };

        let close_btn = widget::button::icon(icon::from_name("window-close-symbolic"))
            .on_press(ScanProgressMessage::Dismiss);

        widget::Row::new()
            .push(label_element)
            .push(widget::determinate_linear(progress).width(cosmic::iced::Length::Fill))
            .push(close_btn)
            .spacing(spacing)
            .padding(padding)
            .align_y(cosmic::iced::alignment::Vertical::Center)
            .into()
    }
}
