use cosmic::Action;
use cosmic::Element;
use cosmic::Task;
use cosmic::widget;
use read_flow_widgets::NavItem;

use crate::app::ContextView;

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

    /// Returns a nav sidebar item for this page.
    ///
    /// `active` is `true` when this page is the currently displayed page.
    /// Return `None` for pages that contribute only a plain leaf (handled by
    /// the App using the existing nav-bar model entry).
    fn nav_tree(&self, _active: bool) -> Option<NavItem<Self::Message>> {
        None
    }
}
