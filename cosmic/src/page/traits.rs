use cosmic::Action;
use cosmic::Element;
use cosmic::Task;

use crate::app::ContextView;

pub trait Page {
    type Message;

    fn view(&self) -> Element<'_, Self::Message>;
    fn view_header_center(&self) -> Vec<Element<'_, Self::Message>>;
    fn view_header_end(&self) -> Vec<Element<'_, Self::Message>>;
    fn view_context(&self) -> ContextView<'_, Self::Message>;
    fn update(&mut self, message: Self::Message) -> Task<Action<Self::Message>>;
}
