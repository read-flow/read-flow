use golden::golden_test;

#[golden_test(320, 60)]
fn text_hello_world() -> cosmic::Element<'static, ()> {
    cosmic::widget::text("Hello, world!").into()
}

#[golden_test(200, 48)]
fn button_label() -> cosmic::Element<'static, ()> {
    cosmic::widget::button::standard("Click me").into()
}
