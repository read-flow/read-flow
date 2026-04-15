// SPDX-License-Identifier: GPL-3.0-or-later

use cosmic::prelude::*;
use cosmic::task;
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
    Out(ScanProgressOutput),
}

pub struct ScanComponent {
    discovered: u64,
    processed: u64,
    errors: u64,
    active: bool,
}

impl ScanComponent {
    pub fn new() -> Self {
        Self {
            discovered: 0,
            processed: 0,
            errors: 0,
            active: true,
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
                    ScanProgress::FileProcessed { .. } => self.processed += 1,
                    ScanProgress::FileError { .. } => self.errors += 1,
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
            ScanProgressMessage::Out(_) => {
                panic!("{message:?} should be handled by the parent")
            }
        }
    }

    pub fn view(&self) -> Element<'_, ScanProgressMessage> {
        let theme = cosmic::theme::active();
        let spacing = theme.cosmic().space_s();
        let padding = theme.cosmic().space_xs();

        let (label, progress) = if self.active {
            let label = fl!(
                "scan-progress-scanning",
                discovered = self.discovered,
                processed = self.processed
            );
            let max = self.discovered.max(1) as f32;
            (label, self.processed as f32 / max)
        } else {
            let label = fl!(
                "scan-progress-completed",
                discovered = self.discovered,
                processed = self.processed,
                errors = self.errors
            );
            (label, 1.0_f32)
        };

        let close_btn = widget::button::icon(icon::from_name("window-close-symbolic"))
            .on_press(ScanProgressMessage::Dismiss);

        widget::Row::new()
            .push(widget::text(label).width(cosmic::iced::Length::Fixed(400.0)))
            .push(widget::determinate_linear(progress).width(cosmic::iced::Length::Fill))
            .push(close_btn)
            .spacing(spacing)
            .padding(padding)
            .align_y(cosmic::iced::alignment::Vertical::Center)
            .into()
    }
}
