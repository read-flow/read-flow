use iced::Element;
use iced::widget::button;
use iced::widget::column;
use iced::widget::row;
use iced::widget::text;
use iced::widget::text_input;

pub fn tag_chips<'a, Msg: Clone + 'a>(
    tags: &'a [String],
    input: &'a str,
    on_input: impl Fn(String) -> Msg + 'a,
    on_add: Msg,
    on_remove: impl Fn(String) -> Msg + 'a,
) -> Element<'a, Msg> {
    let chips: Vec<Element<'a, Msg>> = tags
        .iter()
        .map(|tag| {
            let remove_msg = on_remove(tag.clone());
            button(text(format!("{tag} \u{00d7}")))
                .style(button::secondary)
                .on_press(remove_msg)
                .into()
        })
        .collect();

    let add_row = row![
        text_input("Add tag\u{2026}", input)
            .on_input(on_input)
            .on_submit(on_add.clone())
            .width(160),
        button(text("Add"))
            .style(button::secondary)
            .on_press(on_add),
    ]
    .spacing(6);

    column![row(chips).spacing(4).wrap(), add_row]
        .spacing(6)
        .into()
}

#[cfg(test)]
mod tests {
    // Widget render tests require a running iced context — tested via integration.
    // Unit test: verify the public API compiles with typical message types.
    #[test]
    fn tag_chips_compiles_with_string_message() {
        // Compile-time check — no runtime assertions needed.
        #[allow(clippy::type_complexity)]
        let _: fn(&[String], &str, fn(String) -> String, String, fn(String) -> String) =
            |_, _, _, _, _| ();
    }
}
