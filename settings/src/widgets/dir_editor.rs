use iced::Element;
use iced::Length;
use iced::widget::button;
use iced::widget::checkbox;
use iced::widget::column;
use iced::widget::radio;
use iced::widget::row;
use iced::widget::text;
use iced::widget::text_input;
use read_flow_core::ExpandedPath;
use read_flow_core::scan::DirectorySettings;

use super::settings_section::form_card;
use super::tag_chips::tag_chips;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirAction {
    Scan,
    Ignore,
}

#[derive(Debug, Clone)]
pub struct DirForm {
    pub original_key: Option<ExpandedPath>,
    pub path: String,
    pub action: DirAction,
    pub tags: Vec<String>,
    pub tag_input: String,
    pub inherit: bool,
}

impl DirForm {
    pub fn new_empty() -> Self {
        Self {
            original_key: None,
            path: String::new(),
            action: DirAction::Scan,
            tags: Vec::new(),
            tag_input: String::new(),
            inherit: false,
        }
    }

    pub fn from_entry(key: ExpandedPath, settings: &DirectorySettings) -> Self {
        let (action, tags, inherit) = match settings {
            DirectorySettings::Scan { tags, inherit } => (DirAction::Scan, tags.clone(), *inherit),
            DirectorySettings::Ignore { inherit } => (DirAction::Ignore, vec![], *inherit),
        };
        Self {
            original_key: Some(key.clone()),
            path: key.to_string(),
            action,
            tags,
            tag_input: String::new(),
            inherit,
        }
    }

    pub fn to_directory_settings(&self) -> DirectorySettings {
        match self.action {
            DirAction::Scan => DirectorySettings::Scan {
                tags: self.tags.clone(),
                inherit: self.inherit,
            },
            DirAction::Ignore => DirectorySettings::Ignore {
                inherit: self.inherit,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub enum DirFormMessage {
    PathChanged(String),
    ActionChanged(DirAction),
    TagInput(String),
    AddTag,
    RemoveTag(String),
    InheritToggled(bool),
}

pub fn view_dir_form<'a, Msg: Clone + 'a>(
    form: &'a DirForm,
    wrap: impl Fn(DirFormMessage) -> Msg + Clone + 'a,
    on_browse: Msg,
    on_save: Msg,
    on_cancel: Msg,
) -> Element<'a, Msg> {
    let path_row = row![
        text_input("Directory path\u{2026}", &form.path)
            .on_input({
                let wrap = wrap.clone();
                move |s| wrap(DirFormMessage::PathChanged(s))
            })
            .width(Length::Fill),
        button(text("Browse\u{2026}"))
            .style(button::secondary)
            .on_press(on_browse),
    ]
    .spacing(8)
    .width(Length::Fill);

    let action_row = row![
        text("Action:").width(80),
        radio("Scan", DirAction::Scan, Some(form.action), {
            let wrap = wrap.clone();
            move |a| wrap(DirFormMessage::ActionChanged(a))
        }),
        radio("Ignore", DirAction::Ignore, Some(form.action), {
            let wrap = wrap.clone();
            move |a| wrap(DirFormMessage::ActionChanged(a))
        }),
    ]
    .spacing(12)
    .align_y(iced::Alignment::Center);

    let inherit_row = checkbox(form.inherit)
        .label("Inherit parent tags")
        .on_toggle({
            let wrap = wrap.clone();
            move |b| wrap(DirFormMessage::InheritToggled(b))
        });

    let tags_section: Element<'a, Msg> = if form.action == DirAction::Scan {
        column![
            text("Tags:"),
            tag_chips(
                &form.tags,
                &form.tag_input,
                {
                    let wrap = wrap.clone();
                    move |s| wrap(DirFormMessage::TagInput(s))
                },
                wrap.clone()(DirFormMessage::AddTag),
                {
                    let wrap = wrap.clone();
                    move |t| wrap(DirFormMessage::RemoveTag(t))
                },
            ),
        ]
        .spacing(4)
        .into()
    } else {
        iced::widget::Space::new().into()
    };

    let buttons = row![
        button(text("Save"))
            .style(button::primary)
            .on_press(on_save),
        button(text("Cancel"))
            .style(button::secondary)
            .on_press(on_cancel),
    ]
    .spacing(8);

    let inner = column![path_row, action_row, inherit_row, tags_section, buttons].spacing(8);

    form_card(inner)
}

#[cfg(test)]
mod tests {
    use read_flow_core::scan::DirectorySettings;

    use super::*;

    #[test]
    fn dir_form_scan_roundtrip() {
        let settings = DirectorySettings::Scan {
            tags: vec!["rust".into(), "code".into()],
            inherit: true,
        };
        let key: ExpandedPath = "/home/user/code".parse().unwrap();
        let form = DirForm::from_entry(key, &settings);
        let result = form.to_directory_settings();
        assert_eq!(settings, result);
    }

    #[test]
    fn dir_form_ignore_roundtrip() {
        let settings = DirectorySettings::Ignore { inherit: false };
        let key: ExpandedPath = "/tmp".parse().unwrap();
        let form = DirForm::from_entry(key, &settings);
        let result = form.to_directory_settings();
        assert_eq!(settings, result);
    }

    #[test]
    fn new_empty_form_defaults_to_scan() {
        let form = DirForm::new_empty();
        assert_eq!(form.action, DirAction::Scan);
        assert!(form.path.is_empty());
    }
}
