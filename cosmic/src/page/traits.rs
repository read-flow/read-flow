use cosmic::Action;
use cosmic::Element;
use cosmic::Task;
use cosmic::widget;
use cosmic::widget::icon::Icon;

use crate::app::ContextView;

pub struct NavSubEntry {
    pub label: String,
    pub icon: Option<Icon>,
    pub indent: u16,
}

pub trait Page {
    type Message;

    fn view(&self) -> Element<'_, Self::Message>;
    fn update(&mut self, message: Self::Message) -> Task<Action<Self::Message>>;
    fn view_header_start(&self) -> Vec<Element<'_, Self::Message>> {
        Default::default()
    }
    fn view_header_center(&self) -> Vec<Element<'_, Self::Message>> {
        Default::default()
    }
    fn view_header_end(&self) -> Vec<Element<'_, Self::Message>> {
        Default::default()
    }
    fn view_context(&self) -> ContextView<'_, Self::Message> {
        ContextView {
            title: "Context".to_string(),
            content: widget::text("TODO").into(),
        }
    }
    fn dialog(&self) -> Option<Element<'_, Self::Message>> {
        None
    }
    fn nav_sub_entries(&self) -> Vec<NavSubEntry> {
        vec![]
    }
    fn active_nav_sub_entry(&self) -> Option<usize> {
        None
    }
    fn on_nav_sub_entry_selected(&mut self, _index: usize) -> Task<Action<Self::Message>> {
        Task::none()
    }
}
