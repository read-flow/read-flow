use iced::Element;
use iced::widget::button;
use iced::widget::checkbox;
use iced::widget::column;
use iced::widget::row;
use iced::widget::rule;
use iced::widget::text;
use iced::widget::text_input;
use iced::widget::toggler;
use read_flow_core::scan::DocumentType;
use read_flow_core::scan::ScanSettings;

use crate::app::Message;
use crate::widgets::auto_tags::AutoTagForm;
use crate::widgets::auto_tags::view_auto_tag_form;
use crate::widgets::dir_editor::DirForm;
use crate::widgets::dir_editor::view_dir_form;

pub fn view_scan<'a>(
    scan: &'a ScanSettings,
    dir_form: Option<&'a DirForm>,
    auto_tag_form: Option<&'a AutoTagForm>,
    concurrency_input: &'a str,
) -> Element<'a, Message> {
    let dry_run = toggler(scan.dry_run)
        .label("Dry run (scan without writing to database)")
        .on_toggle(Message::ToggleDryRun);

    let concurrency_row = row![
        text("Concurrency:").width(130),
        text_input("16", concurrency_input)
            .on_input(Message::ConcurrencyChanged)
            .width(80),
        text("parallel workers"),
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center);

    let all_enabled = DocumentType::all()
        .iter()
        .all(|dt| scan.extensions.contains(dt));
    let all_toggle = checkbox(all_enabled)
        .label("Select all")
        .on_toggle(Message::ToggleAllExtensions);

    let ext_checkboxes: Vec<Element<'a, Message>> = DocumentType::all()
        .iter()
        .map(|dt| {
            let enabled = scan.extensions.contains(dt);
            let dt_clone = dt.clone();
            checkbox(enabled)
                .label(dt.label())
                .on_toggle(move |b| Message::ToggleExtension(dt_clone.clone(), b))
                .into()
        })
        .collect();

    let extensions_section = column![
        text("File types:").size(14),
        all_toggle,
        column(ext_checkboxes).spacing(4),
    ]
    .spacing(6);

    let dirs_section = view_directories_section(scan, dir_form);
    let auto_tags_section = view_auto_tags_section(scan, auto_tag_form);

    column![
        text("Scan").size(20),
        text("Configure how the file system scan works.").size(13),
        rule::horizontal(1),
        dry_run,
        concurrency_row,
        rule::horizontal(1),
        extensions_section,
        rule::horizontal(1),
        dirs_section,
        rule::horizontal(1),
        auto_tags_section,
    ]
    .spacing(12)
    .padding(20)
    .into()
}

fn view_directories_section<'a>(
    scan: &'a ScanSettings,
    dir_form: Option<&'a DirForm>,
) -> Element<'a, Message> {
    let adding = dir_form.map(|f| f.original_key.is_none()).unwrap_or(false);

    let mut dir_rows: Vec<Element<'a, Message>> = scan
        .directories
        .iter()
        .flat_map(|(path, settings)| {
            let key = path.clone();
            let key2 = path.clone();
            let is_editing = dir_form
                .and_then(|f| f.original_key.as_ref())
                .map(|k| k == path)
                .unwrap_or(false);

            let action_label = match settings {
                read_flow_core::scan::DirectorySettings::Scan { .. } => "Scan",
                read_flow_core::scan::DirectorySettings::Ignore { .. } => "Ignore",
            };

            let header: Element<'a, Message> = row![
                text(path.to_string()).width(iced::Fill),
                text(action_label).width(60),
                button(text("Edit"))
                    .style(button::secondary)
                    .on_press(Message::DirEditStart(key)),
                button(text("Remove"))
                    .style(button::danger)
                    .on_press(Message::DirRemove(key2)),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center)
            .into();

            if is_editing {
                vec![
                    header,
                    view_dir_form(
                        dir_form.unwrap(),
                        Message::DirForm,
                        Message::DirBrowse,
                        Message::DirSave,
                        Message::DirCancel,
                    ),
                ]
            } else {
                vec![header]
            }
        })
        .collect();

    if adding {
        dir_rows.push(view_dir_form(
            dir_form.unwrap(),
            Message::DirForm,
            Message::DirBrowse,
            Message::DirSave,
            Message::DirCancel,
        ));
    }

    dir_rows.push(
        button(text("+ Add Directory"))
            .style(button::secondary)
            .on_press(Message::DirAddStart)
            .into(),
    );

    column![
        text("Scan directories:").size(14),
        column(dir_rows).spacing(6)
    ]
    .spacing(6)
    .into()
}

fn view_auto_tags_section<'a>(
    scan: &'a ScanSettings,
    auto_tag_form: Option<&'a AutoTagForm>,
) -> Element<'a, Message> {
    let adding = auto_tag_form
        .map(|f| f.original_key.is_none())
        .unwrap_or(false);

    let mut rows: Vec<Element<'a, Message>> = scan
        .auto_tags
        .iter()
        .flat_map(|(pattern, tags)| {
            let key = pattern.clone();
            let key2 = pattern.clone();
            let is_editing = auto_tag_form
                .and_then(|f| f.original_key.as_ref())
                .map(|k| k == pattern)
                .unwrap_or(false);

            let tags_display = tags.join(", ");
            let header: Element<'a, Message> = row![
                text(pattern.clone()).width(iced::Fill),
                text(format!("\u{2192} {tags_display}")).width(iced::Fill),
                button(text("Edit"))
                    .style(button::secondary)
                    .on_press(Message::AutoTagEditStart(key)),
                button(text("Remove"))
                    .style(button::danger)
                    .on_press(Message::AutoTagRemove(key2)),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center)
            .into();

            if is_editing {
                vec![
                    header,
                    view_auto_tag_form(
                        auto_tag_form.unwrap(),
                        Message::AutoTagForm,
                        Message::AutoTagSave,
                        Message::AutoTagCancel,
                    ),
                ]
            } else {
                vec![header]
            }
        })
        .collect();

    if adding {
        rows.push(view_auto_tag_form(
            auto_tag_form.unwrap(),
            Message::AutoTagForm,
            Message::AutoTagSave,
            Message::AutoTagCancel,
        ));
    }

    rows.push(
        button(text("+ Add Auto-tag Rule"))
            .style(button::secondary)
            .on_press(Message::AutoTagAddStart)
            .into(),
    );

    column![text("Auto-tag rules:").size(14), column(rows).spacing(6)]
        .spacing(6)
        .into()
}
