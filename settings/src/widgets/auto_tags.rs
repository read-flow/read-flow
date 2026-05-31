use iced::Element;
use iced::Length;
use iced::widget::button;
use iced::widget::column;
use iced::widget::row;
use iced::widget::text;
use iced::widget::text_input;

use super::settings_section::form_card;
use super::tag_chips::tag_chips;

#[derive(Debug, Clone)]
pub struct AutoTagForm {
    pub original_key: Option<String>,
    pub pattern: String,
    pub tags: Vec<String>,
    pub tag_input: String,
}

impl AutoTagForm {
    pub fn new_empty() -> Self {
        Self {
            original_key: None,
            pattern: String::new(),
            tags: Vec::new(),
            tag_input: String::new(),
        }
    }

    pub fn from_entry(key: String, tags: Vec<String>) -> Self {
        Self {
            original_key: Some(key.clone()),
            pattern: key,
            tags,
            tag_input: String::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum AutoTagFormMessage {
    PatternChanged(String),
    TagInput(String),
    AddTag,
    RemoveTag(String),
}

pub fn view_auto_tag_form<'a, Msg: Clone + 'a>(
    form: &'a AutoTagForm,
    wrap: impl Fn(AutoTagFormMessage) -> Msg + Clone + 'a,
    on_save: Msg,
    on_cancel: Msg,
) -> Element<'a, Msg> {
    let pattern_row: Element<'a, Msg> = if form.original_key.is_none() {
        row![
            text("Pattern:").width(80),
            text_input("file pattern or regex\u{2026}", &form.pattern)
                .on_input({
                    let wrap = wrap.clone();
                    move |s| wrap(AutoTagFormMessage::PatternChanged(s))
                })
                .width(Length::Fill),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center)
        .width(Length::Fill)
        .into()
    } else {
        text(format!("Pattern: {}", form.pattern)).into()
    };

    let tags_section = column![
        text("Tags:"),
        tag_chips(
            &form.tags,
            &form.tag_input,
            {
                let wrap = wrap.clone();
                move |s| wrap(AutoTagFormMessage::TagInput(s))
            },
            wrap.clone()(AutoTagFormMessage::AddTag),
            {
                let wrap = wrap.clone();
                move |t| wrap(AutoTagFormMessage::RemoveTag(t))
            },
        ),
    ]
    .spacing(4);

    let buttons = row![
        button(text("Save"))
            .style(button::primary)
            .on_press(on_save),
        button(text("Cancel"))
            .style(button::secondary)
            .on_press(on_cancel),
    ]
    .spacing(8);

    let inner = column![pattern_row, tags_section, buttons].spacing(8);

    form_card(inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_entry_preserves_tags() {
        let form = AutoTagForm::from_entry("*.epub".into(), vec!["book".into(), "reading".into()]);
        assert_eq!(form.pattern, "*.epub");
        assert_eq!(form.tags, vec!["book", "reading"]);
    }

    #[test]
    fn new_empty_has_no_pattern_or_tags() {
        let form = AutoTagForm::new_empty();
        assert!(form.pattern.is_empty());
        assert!(form.tags.is_empty());
    }
}
